// 运营商信息数据库 (MCC-MNC 映射)
export interface CarrierInfo {
  mccMnc: string
  mcc: string
  mnc: string
  operatorCn: string // 中文名称
  operatorEn: string // 英文名称
  brand: string // 品牌
  status: string // 状态
  technology?: string // 技术标准
  notes?: string // 备注
}

// 中国大陆运营商映射表
export const CHINA_CARRIERS: CarrierInfo[] = [
  {
    mccMnc: '46000',
    mcc: '460',
    mnc: '00',
    operatorCn: '中国移动',
    operatorEn: 'China Mobile',
    brand: '中国移动',
    status: '营运中',
    technology: 'GSM 900 / GSM 1800 / TD-SCDMA 1880 / TD-SCDMA 2010 / TD-LTE 1800/2300/2600',
  },
  {
    mccMnc: '46001',
    mcc: '460',
    mnc: '01',
    operatorCn: '中国联通',
    operatorEn: 'China Unicom',
    brand: '中国联通',
    status: '营运中',
    technology: 'GSM 900 / GSM 1800 / UMTS 2100 / TD-LTE 2300/2600 / FDD-LTE 1800/2100',
  },
  {
    mccMnc: '46002',
    mcc: '460',
    mnc: '02',
    operatorCn: '中国移动',
    operatorEn: 'China Mobile',
    brand: '中国移动',
    status: '营运中',
    technology: 'GSM 900 / GSM 1800 / TD-SCDMA 1880 / TD-SCDMA 2010',
  },
  {
    mccMnc: '46004',
    mcc: '460',
    mnc: '04',
    operatorCn: '中国移动',
    operatorEn: 'China Mobile',
    brand: '中国移动',
    status: '营运中',
  },
  {
    mccMnc: '46003',
    mcc: '460',
    mnc: '03',
    operatorCn: '中国电信',
    operatorEn: 'China Telecom',
    brand: '中国电信',
    status: '营运中',
    technology: 'CDMA2000 800 / CDMA2000 2100 / TD-LTE 2300/2600 / FDD-LTE 1800/2100 / EV-DO / eHRPD',
  },
  {
    mccMnc: '46005',
    mcc: '460',
    mnc: '05',
    operatorCn: '中国电信',
    operatorEn: 'China Telecom',
    brand: '中国电信',
    status: '营运中',
  },
  {
    mccMnc: '46006',
    mcc: '460',
    mnc: '06',
    operatorCn: '中国联通',
    operatorEn: 'China Unicom',
    brand: '中国联通',
    status: '营运中',
    technology: 'GSM 900 / GSM 1800 / UMTS 2100',
  },
  {
    mccMnc: '46007',
    mcc: '460',
    mnc: '07',
    operatorCn: '中国移动',
    operatorEn: 'China Mobile',
    brand: '中国移动',
    status: '营运中',
    technology: 'GSM 900 / GSM 1800 / TD-SCDMA 1880 / TD-SCDMA 2010',
  },
  {
    mccMnc: '46008',
    mcc: '460',
    mnc: '08',
    operatorCn: '中国移动',
    operatorEn: 'China Mobile',
    brand: '中国移动',
    status: '营运中',
  },
  {
    mccMnc: '46009',
    mcc: '460',
    mnc: '09',
    operatorCn: '中国联通',
    operatorEn: 'China Unicom',
    brand: '中国联通',
    status: '营运中',
  },
  {
    mccMnc: '46010',
    mcc: '460',
    mnc: '10',
    operatorCn: '中国联通',
    operatorEn: 'China Unicom',
    brand: '中国联通',
    status: '营运中',
  },
  {
    mccMnc: '46011',
    mcc: '460',
    mnc: '11',
    operatorCn: '中国电信',
    operatorEn: 'China Telecom',
    brand: '中国电信',
    status: '营运中',
    technology: 'CDMA2000 800 / CDMA2000 2100 / TD-LTE 2300/2600 / FDD-LTE 1800/2100 / EV-DO / eHRPD',
  },
  {
    mccMnc: '46012',
    mcc: '460',
    mnc: '12',
    operatorCn: '中国电信',
    operatorEn: 'China Telecom',
    brand: '中国电信',
    status: '营运中',
  },
  {
    mccMnc: '46015',
    mcc: '460',
    mnc: '15',
    operatorCn: '中国广电',
    operatorEn: 'China Broadnet',
    brand: '中国广电',
    status: '营运中',
    technology: 'LTE 1800 / LTE 900 / TD-LTE 1900 / TD-LTE 2300 / 5G 700 / 5G 2500',
  },
  {
    mccMnc: '46020',
    mcc: '460',
    mnc: '20',
    operatorCn: '中国铁通',
    operatorEn: 'China Tietong',
    brand: '中国铁通',
    status: '营运中',
    technology: 'GSM-R',
  },
]

// 创建快速查找映射
const carrierMap = new Map<string, CarrierInfo>()
CHINA_CARRIERS.forEach((carrier) => {
  carrierMap.set(carrier.mccMnc, carrier)
})

/**
 * 根据 MCC 和 MNC 获取运营商信息
 * @param mcc 移动国家代码
 * @param mnc 移动网络代码
 * @returns 运营商信息或 null
 */
export function getCarrierInfo(mcc: string | number | undefined, mnc: string | number | undefined): CarrierInfo | null {
  if (!mcc || !mnc) return null

  // 标准化 MCC 和 MNC (补零)
  const mccStr = String(mcc).padStart(3, '0')
  const mncStr = String(mnc).padStart(2, '0')
  const mccMnc = `${mccStr}${mncStr}`

  return carrierMap.get(mccMnc) || null
}

/**
 * 格式化运营商显示名称
 * @param mcc 移动国家代码
 * @param mnc 移动网络代码
 * @param showEnglish 是否显示英文名称
 * @returns 格式化的运营商名称
 */
export function formatCarrierName(
  mcc: string | number | undefined,
  mnc: string | number | undefined,
  showEnglish = false
): string {
  const carrier = getCarrierInfo(mcc, mnc)
  
  if (!carrier) {
    // 如果找不到对应的运营商，返回原始 MCC-MNC
    if (mcc && mnc) {
      return `${mcc}-${mnc}`
    }
    return 'Unknown'
  }

  if (showEnglish) {
    return `${carrier.operatorCn} (${carrier.operatorEn})`
  }

  return carrier.operatorCn
}

/**
 * 获取运营商品牌颜色 (用于 UI 显示)
 * @param mcc 移动国家代码
 * @param mnc 移动网络代码
 * @returns MUI Chip 颜色
 */
export function getCarrierColor(mcc: string | number | undefined, mnc: string | number | undefined): 
  'default' | 'primary' | 'secondary' | 'error' | 'info' | 'success' | 'warning' {
  if (!mcc || !mnc) return 'default'
  
  const mccStr = String(mcc)
  const mncStr = String(mnc).padStart(2, '0')
  
  // 中国大陆运营商 (MCC 460)
  if (mccStr === '460') {
    switch (mncStr) {
      // 中国移动: 00, 02, 04, 07, 08, 16, 20 - 绿色
      case '00':
      case '02':
      case '04':
      case '07':
      case '08':
      case '16':
      case '20':
        return 'success'
      // 中国联通: 01, 06, 09, 10 - 红色
      case '01':
      case '06':
      case '09':
      case '10':
        return 'error'
      // 中国电信: 03, 05, 11, 12 - 蓝色
      case '03':
      case '05':
      case '11':
      case '12':
        return 'primary'
      // 中国广电: 15 - 紫色
      case '15':
        return 'secondary'
    }
  }
  
  return 'default'
}

/**
 * 获取运营商 Logo 路径
 * @param mcc 移动国家代码
 * @param mnc 移动网络代码
 * @returns Logo SVG 路径，找不到则返回 null
 */
export function getCarrierLogo(mcc: string | number | undefined, mnc: string | number | undefined): string | null {
  if (!mcc || !mnc) return null
  
  const mccStr = String(mcc)
  const mncStr = String(mnc).padStart(2, '0')
  
  // 中国大陆运营商 (MCC 460)
  if (mccStr === '460') {
    switch (mncStr) {
      // 中国移动: 00, 02, 04, 07, 08, 16, 20
      case '00':
      case '02':
      case '04':
      case '07':
      case '08':
      case '16':
      case '20':
        return '/provider/china-mobile.svg'
      // 中国联通: 01, 06, 09, 10
      case '01':
      case '06':
      case '09':
      case '10':
        return '/provider/china-unicom.svg'
      // 中国电信: 03, 05, 11, 12
      case '03':
      case '05':
      case '11':
      case '12':
        return '/provider/china-telecom.svg'
      // 中国广电: 15
      case '15':
        return '/provider/china-broadnet.svg'
    }
  }
  
  return null
}
