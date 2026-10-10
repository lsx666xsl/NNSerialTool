// 插件系统配置：市场源、持久化键名。
// 市场源可在插件管理面板里覆盖（存 localStorage），默认指向官方插件仓库。
import { readStorage, writeStorage } from '../utils/storage';

export const MARKET_SOURCES = [
  'https://cdn.jsdelivr.net/gh/lsx666xsl/NNSerialTool-Plugins@main/index.json',
  'https://raw.githubusercontent.com/lsx666xsl/NNSerialTool-Plugins/main/index.json',
];

export const MARKET_SOURCE_KEY = 'st-plugin-market-url';

export const readMarketSource = (): string => readStorage<string>(MARKET_SOURCE_KEY, '') || MARKET_SOURCES[0];

export const writeMarketSource = (url: string) => writeStorage(MARKET_SOURCE_KEY, url.trim());
