#!/usr/bin/env node

/**
 * SimAdmin Universal Startup & Registration Benchmark Tool
 * 
 * 通用启动与驻网时延基准测试工具
 * 适用对象：dev 标准版 vs volte 版，或任意两台 SimAdmin 设备
 * 
 * 功能：
 * 1. 毫秒级监控设备重启/启动过程中的各关键里程碑：
 *    - T0: 重启触发 / 设备下线
 *    - T1: Web 服务就绪 (GET /api/health HTTP 200)
 *    - T2: 仪表盘 SIM 卡信息识别 (GET /api/sim 返回有效 ICCID / 运营商)
 *    - T3: 蜂窝网络搜网与驻网 (GET /api/network 返回 registered / roaming)
 *    - T4: VoLTE / IMS 注册完成 (若固件支持 VoLTE)
 * 2. 自动生成时延指标评分卡，并可导出 JSON 结果供对比分析。
 * 3. 支持跨版本对比模式：--compare dev.json volte.json
 *
 * 用法：
 *   node scripts/benchmark-startup.mjs http://192.168.66.1:3000
 *   node scripts/benchmark-startup.mjs http://192.168.68.1:3000 --reboot
 *   node scripts/benchmark-startup.mjs --compare dev.json volte.json
 */

import fs from 'node:fs'
import path from 'node:path'

const colors = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  dim: '\x1b[2m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  red: '\x1b[31m',
}

const args = process.argv.slice(2)

// 对比模式
const compareIdx = args.indexOf('--compare')
if (compareIdx !== -1 && args[compareIdx + 1] && args[compareIdx + 2]) {
  runComparison(args[compareIdx + 1], args[compareIdx + 2])
  process.exit(0)
}

let targetUrl = 'http://192.168.68.1:3000'
let doReboot = false
let monitorOnly = false
let pollIntervalMs = 500
let customOutputFile = null

for (const arg of args) {
  if (arg.startsWith('http://') || arg.startsWith('https://')) {
    targetUrl = arg.replace(/\/+$/, '')
  } else if (arg === '--reboot') {
    doReboot = true
  } else if (arg === '--monitor-only' || arg === '--now') {
    monitorOnly = true
  } else if (arg.startsWith('--interval=')) {
    pollIntervalMs = parseInt(arg.split('=')[1], 10) || 500
  } else if (arg.startsWith('--output=')) {
    customOutputFile = arg.split('=')[1].trim()
  }
}

function log(msg, color = colors.reset) {
  const time = new Date().toLocaleTimeString('zh-CN', { hour12: false })
  console.log(`${colors.dim}[${time}]${colors.reset} ${color}${msg}${colors.reset}`)
}

async function fetchWithTimeout(url, options = {}, timeoutMs = 2000) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const res = await fetch(url, { ...options, signal: controller.signal })
    clearTimeout(timer)
    return res
  } catch (err) {
    clearTimeout(timer)
    return null
  }
}

async function checkHealth(baseUrl) {
  const res = await fetchWithTimeout(`${baseUrl}/api/health`, {}, 1500)
  if (res && res.status === 200) {
    try {
      const data = await res.json()
      return { ok: true, version: data.version || 'unknown' }
    } catch {
      return { ok: true, version: 'unknown' }
    }
  }
  return { ok: false }
}

async function checkSim(baseUrl) {
  const res = await fetchWithTimeout(`${baseUrl}/api/sim`, {}, 1500)
  if (res && res.status === 200) {
    try {
      const json = await res.json()
      const data = json.data || json
      if (data && (data.iccid || data.imsi || data.operator_name || data.present)) {
        return {
          ok: Boolean(data.iccid || data.imsi),
          iccid: data.iccid || '',
          imsi: data.imsi || '',
          operator: data.registered_operator_name || data.operator_name || '未知运营商',
          present: data.present ?? true,
        }
      }
    } catch { }
  }
  return { ok: false }
}

async function checkNetwork(baseUrl) {
  const res = await fetchWithTimeout(`${baseUrl}/api/network`, {}, 1500)
  if (res && res.status === 200) {
    try {
      const json = await res.json()
      const data = json.data || json
      if (data && data.registration_status) {
        const reg = data.registration_status.toLowerCase()
        const isRegistered = reg === 'registered' || reg === 'roaming'
        return {
          ok: isRegistered,
          status: data.registration_status,
          tech: data.technology_preference || 'LTE',
          signal: data.signal_strength ?? 0,
          operator: data.operator_name || '',
        }
      }
    } catch { }
  }
  return { ok: false }
}

async function checkVolte(baseUrl) {
  const res = await fetchWithTimeout(`${baseUrl}/api/volte/control`, {}, 1500)
  if (res && res.status === 200) {
    try {
      const json = await res.json()
      const data = json.data || json
      const runtime = data.runtime
      const config = data.config
      if (runtime) {
        const featureEnabled = config?.feature_enabled ?? true
        const connectionEnabled = config?.connection_enabled ?? true
        return {
          supported: true,
          featureEnabled,
          connectionEnabled,
          registered: Boolean(runtime.registered),
          phase: runtime.phase || 'disabled',
          ip: runtime.assigned_ip || '',
        }
      }
    } catch { }
  }
  return { supported: false, registered: false, phase: 'unsupported' }
}

async function triggerReboot(baseUrl) {
  log(`📡 正在向 ${baseUrl}/api/system/reboot 发送重启指令...`, colors.yellow)
  try {
    const res = await fetchWithTimeout(`${baseUrl}/api/system/reboot`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ delay_seconds: 1 }),
    }, 3000)
    if (res && res.status === 200) {
      log(`✅ 重启指令发送成功，设备即将重启...`, colors.green)
      return true
    }
  } catch { }
  log(`⚠️ 自动重启接口未响应（可能是 dev 标准版或未登录），转为监听离线事件...`, colors.yellow)
  return false
}

async function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

async function main() {
  console.log('')
  console.log(`${colors.cyan}========================================================================${colors.reset}`)
  console.log(`${colors.bright}          SimAdmin 启动时延与驻网性能基准监控工具 (Universal Benchmark)          ${colors.reset}`)
  console.log(`${colors.cyan}========================================================================${colors.reset}`)
  console.log(`目标设备: ${colors.green}${targetUrl}${colors.reset}`)
  console.log(`轮询频率: ${pollIntervalMs}ms`)
  console.log(`运行模式: ${doReboot ? '自动重启测试' : monitorOnly ? '立即计时测试' : '智能监听重启测试'}`)
  console.log('')

  let t0 = 0
  let tHealth = null
  let tSim = null
  let tNet = null
  let tVolte = null
  let simInfo = null
  let netInfo = null
  let volteInfo = null
  let deviceVersion = 'unknown'

  // 1. 检查当前是否在线
  const initialHealth = await checkHealth(targetUrl)
  if (initialHealth.ok) {
    deviceVersion = initialHealth.version
    log(`🟢 设备当前在线 (固件版本: ${deviceVersion})`, colors.green)
  } else {
    log(`🟡 设备当前处于离线状态，将直接等待开机上线...`, colors.yellow)
  }

  if (doReboot) {
    await triggerReboot(targetUrl)
    log(`⏳ 等待设备完成关机并进入离线状态...`, colors.cyan)
    // 连续 2 次探测失败，确认为真正离线断开
    let offlineCount = 0
    while (offlineCount < 2) {
      const h = await checkHealth(targetUrl)
      if (!h.ok) {
        offlineCount++
      } else {
        offlineCount = 0
      }
      await sleep(500)
    }
    t0 = Date.now()
    log(`🔴 设备已确认离线，基准计时器已启动 (T0 = 0.0s)`, colors.red)
  } else if (!monitorOnly && initialHealth.ok) {
    log(`💡 请在 Web 仪表盘点击【重启设备】，或在终端执行 reboot；程序将自动开始计时...`, colors.magenta)
    let offlineCount = 0
    while (offlineCount < 2) {
      const h = await checkHealth(targetUrl)
      if (!h.ok) {
        offlineCount++
      } else {
        offlineCount = 0
      }
      await sleep(500)
    }
    t0 = Date.now()
    log(`🔴 检测到设备已离线重启，基准计时器已启动 (T0 = 0.0s)`, colors.red)
  } else {
    t0 = Date.now()
    log(`⏱️ 从当前时刻开始计时 (T0 = 0.0s)...`, colors.cyan)
  }

  log(`⏳ 正在持续探测系统启动里程碑...`, colors.cyan)

  // 持续轮询直到关键指标达成
  const maxWaitMs = 180_000 // 最大等 3 分钟
  while (Date.now() - t0 < maxWaitMs) {
    const elapsedSec = ((Date.now() - t0) / 1000).toFixed(1)

    // 1. 探测 Web 服务上线
    if (tHealth === null) {
      const h = await checkHealth(targetUrl)
      if (h.ok) {
        tHealth = Date.now()
        deviceVersion = h.version
        log(`🟢 [+${elapsedSec}s] 【里程碑 1】Web 服务就绪 (GET /api/health -> 200, v${deviceVersion})`, colors.green)
      }
    }

    // 2. 探测 SIM 卡信息识别
    if (tHealth !== null && tSim === null) {
      const sim = await checkSim(targetUrl)
      if (sim.ok) {
        tSim = Date.now()
        simInfo = sim
        log(`💳 [+${elapsedSec}s] 【里程碑 2】仪表盘 SIM 卡信息识别完成！`, colors.green)
        log(`   - 运营商: ${sim.operator} | ICCID: ${sim.iccid.slice(0, 10)}... | IMSI: ${sim.imsi.slice(0, 8)}...`, colors.cyan)
      }
    }

    // 3. 探测蜂窝驻网
    if (tHealth !== null && tNet === null) {
      const net = await checkNetwork(targetUrl)
      if (net.ok) {
        tNet = Date.now()
        netInfo = net
        log(`📶 [+${elapsedSec}s] 【里程碑 3】蜂窝网络驻网成功！`, colors.green)
        log(`   - 状态: ${net.status} (${net.operator}) | 制式: ${net.tech} | 信号强度: ${net.signal}%`, colors.cyan)
      }
    }

    // 4. 探测 VoLTE (若支持)
    if (tNet !== null && tVolte === null) {
      const v = await checkVolte(targetUrl)
      if (v.supported) {
        if (v.registered) {
          tVolte = Date.now()
          volteInfo = v
          log(`📞 [+${elapsedSec}s] 【里程碑 4】VoLTE / IMS 注册就绪！`, colors.green)
          log(`   - 状态: ${v.phase} | IMS IP: ${v.ip || '已分配'}`, colors.cyan)
        } else if (!v.featureEnabled || !v.connectionEnabled || v.phase === 'disabled') {
          tVolte = false
          volteInfo = v
          log(`📞 [+${elapsedSec}s] VoLTE 服务处于未启用状态`, colors.yellow)
        }
      } else {
        // 不支持 VoLTE（dev 标准版）
        tVolte = false
      }
    }

    // 如果全部核心指标达成（且 VoLTE 已判定或驻网后等待超过 15 秒），结束并输出报告
    if (tHealth !== null && tSim !== null && tNet !== null) {
      if (tVolte !== null || (tNet && (Date.now() - tNet) > 15_000)) {
        break
      }
    }

    await sleep(pollIntervalMs)
  }

  // 打印总结报告
  printReport({
    targetUrl,
    deviceVersion,
    t0,
    tHealth,
    tSim,
    tNet,
    tVolte,
    simInfo,
    netInfo,
    volteInfo,
  })
}

function printReport(data) {
  const { targetUrl, deviceVersion, t0, tHealth, tSim, tNet, tVolte, simInfo, netInfo, volteInfo } = data

  const healthSec = tHealth ? ((tHealth - t0) / 1000) : null
  const simSec = tSim ? ((tSim - t0) / 1000) : null
  const netSec = tNet ? ((tNet - t0) / 1000) : null
  const volteSec = (tVolte && typeof tVolte === 'number') ? ((tVolte - t0) / 1000) : null

  const simDelta = (simSec !== null && healthSec !== null) ? (simSec - healthSec) : null
  const netDelta = (netSec !== null && healthSec !== null) ? (netSec - healthSec) : null

  console.log('')
  console.log(`${colors.cyan}========================================================================${colors.reset}`)
  console.log(`${colors.bright}                     SimAdmin 启动时延性能基准报告                      ${colors.reset}`)
  console.log(`${colors.cyan}========================================================================${colors.reset}`)
  console.log(`目标地址: ${targetUrl}`)
  console.log(`固件版本: v${deviceVersion}`)
  console.log(`测试时间: ${new Date().toLocaleString('zh-CN')}`)
  console.log(`${colors.dim}------------------------------------------------------------------------${colors.reset}`)
  console.log(`阶段指标                 达成耗时(相对T0)   服务就绪后增量   性能评级`)
  console.log(`${colors.dim}------------------------------------------------------------------------${colors.reset}`)

  const fmt = (sec) => (sec !== null ? `${sec.toFixed(1)}s`.padEnd(8) : '超时/未完成'.padEnd(8))
  const fmtDelta = (delta) => (delta !== null ? `+${delta.toFixed(1)}s`.padEnd(10) : '-'.padEnd(10))

  // 1. Web 就绪
  console.log(`1. Web 核心服务上线       ${fmt(healthSec)}         -                 ${healthSec && healthSec < 20 ? '⚡ 极速' : '正常'}`)

  // 2. SIM 卡识别
  let simRating = '⚠️ 偏慢'
  if (simDelta !== null) {
    simRating = simDelta <= 5 ? '⚡ 极速 (<5s, 达标)' : simDelta <= 15 ? '🟢 正常 (5-15s)' : '⚠️ 偏慢 (>15s)'
  }
  console.log(`2. 仪表盘 SIM 卡识别      ${fmt(simSec)}         ${fmtDelta(simDelta)}        ${simRating}`)

  // 3. 蜂窝网络驻网
  let netRating = '⚠️ 偏慢'
  if (netDelta !== null) {
    netRating = netDelta <= 8 ? '⚡ 极速 (<8s, 对齐dev)' : netDelta <= 20 ? '🟢 正常 (8-20s)' : '⚠️ 偏慢 (>20s)'
  }
  console.log(`3. 蜂窝网络搜网与驻网     ${fmt(netSec)}         ${fmtDelta(netDelta)}        ${netRating}`)

  // 4. VoLTE (如果有)
  if (volteSec !== null) {
    const vDelta = volteSec - (netSec || healthSec || 0)
    console.log(`4. VoLTE / IMS 注册完成   ${fmt(volteSec)}         ${fmtDelta(vDelta)}        ${vDelta <= 10 ? '⚡ 极速' : '正常'}`)
  } else if (tVolte === false) {
    console.log(`4. VoLTE 服务状态         未开启/标准版分支   -                 -`)
  }

  console.log(`${colors.dim}------------------------------------------------------------------------${colors.reset}`)

  // 结论
  if (netDelta !== null && netDelta <= 10) {
    console.log(`${colors.green}${colors.bright}🏆 综合判定: 优秀！开机 SIM 卡识别与驻网速度已完全对齐 dev 标准版基准！${colors.reset}`)
  } else if (netDelta !== null && netDelta <= 25) {
    console.log(`${colors.yellow}👌 综合判定: 良好。驻网在正常时间窗口内完成。${colors.reset}`)
  } else {
    console.log(`${colors.red}⚠️ 综合判定: 驻网耗时过长，可能存在启动项抢占阻塞，请检查开机服务依赖。${colors.reset}`)
  }
  console.log(`${colors.cyan}========================================================================${colors.reset}`)

  // 自动保存基准文件
  const resultObj = {
    targetUrl,
    version: deviceVersion,
    timestamp: new Date().toISOString(),
    metrics: {
      healthSec,
      simSec,
      simDelta,
      netSec,
      netDelta,
      volteSec,
    },
    details: {
      sim: simInfo,
      network: netInfo,
      volte: volteInfo,
    },
  }

  const defaultDir = path.resolve('temp/MonitoringResults')
  if (!fs.existsSync(defaultDir)) {
    fs.mkdirSync(defaultDir, { recursive: true })
  }
  const defaultFilename = path.join(defaultDir, `benchmark-${deviceVersion.replace(/[^a-zA-Z0-9.-]/g, '_')}-${Date.now()}.json`)
  const filename = customOutputFile
    ? (path.isAbsolute(customOutputFile) ? customOutputFile : path.join(defaultDir, path.basename(customOutputFile)))
    : defaultFilename
  try {
    fs.writeFileSync(filename, JSON.stringify(resultObj, null, 2), 'utf-8')
    console.log(`📁 结果已自动保存至: ${colors.cyan}${filename}${colors.reset}`)
    console.log(`   (稍后可使用 node scripts/benchmark-startup.mjs --compare benchmark-standard.json benchmark-volte.json 进行横向对比)`)
  } catch { }
  console.log('')
}

function resolveBenchmarkFile(filePath) {
  if (fs.existsSync(filePath)) return filePath
  const inTemp = path.join('temp/MonitoringResults', filePath)
  if (fs.existsSync(inTemp)) return inTemp
  return filePath
}

function runComparison(fileA, fileB) {
  try {
    const resolvedA = resolveBenchmarkFile(fileA)
    const resolvedB = resolveBenchmarkFile(fileB)
    const a = JSON.parse(fs.readFileSync(resolvedA, 'utf-8'))
    const b = JSON.parse(fs.readFileSync(resolvedB, 'utf-8'))

    console.log('')
    console.log(`${colors.cyan}========================================================================${colors.reset}`)
    console.log(`${colors.bright}               SimAdmin 双版本启动与驻网性能对比报告                    ${colors.reset}`)
    console.log(`${colors.cyan}========================================================================${colors.reset}`)
    console.log(`基准版本 (A): ${colors.yellow}${a.version}${colors.reset} (${path.basename(fileA)})`)
    console.log(`对比版本 (B): ${colors.green}${b.version}${colors.reset} (${path.basename(fileB)})`)
    console.log(`${colors.dim}------------------------------------------------------------------------${colors.reset}`)
    console.log(`指标阶段                   基准版本(A)      对比版本(B)      差异 (B - A)`)
    console.log(`${colors.dim}------------------------------------------------------------------------${colors.reset}`)

    function row(name, valA, valB) {
      const sa = valA !== null && valA !== undefined ? `${valA.toFixed(1)}s`.padEnd(12) : 'N/A'.padEnd(12)
      const sb = valB !== null && valB !== undefined ? `${valB.toFixed(1)}s`.padEnd(12) : 'N/A'.padEnd(12)
      let diff = '-'
      if (valA !== null && valB !== null && valA !== undefined && valB !== undefined) {
        const d = valB - valA
        const sign = d > 0 ? `+${d.toFixed(1)}s (更慢)` : `${d.toFixed(1)}s (更快)`
        const col = d <= 0 ? colors.green : d < 3 ? colors.yellow : colors.red
        diff = `${col}${sign}${colors.reset}`
      }
      console.log(`${name.padEnd(20)} ${sa}     ${sb}     ${diff}`)
    }

    row('1. Web服务就绪(T1)', a.metrics.healthSec, b.metrics.healthSec)
    row('2. SIM卡识别(T2)', a.metrics.simSec, b.metrics.simSec)
    row('   └ 服务上线后耗时', a.metrics.simDelta, b.metrics.simDelta)
    row('3. 蜂窝网络驻网(T3)', a.metrics.netSec, b.metrics.netSec)
    row('   └ 服务上线后耗时', a.metrics.netDelta, b.metrics.netDelta)
    row('4. VoLTE/IMS注册(T4)', a.metrics.volteSec, b.metrics.volteSec)

    console.log(`${colors.dim}------------------------------------------------------------------------${colors.reset}`)
    console.log(`${colors.cyan}========================================================================${colors.reset}`)
    console.log('')
  } catch (err) {
    console.error('对比分析失败:', err.message)
  }
}

main().catch(console.error)
