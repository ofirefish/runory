import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en-US.json";
import zh from "./locales/zh-CN.json";

void i18n.use(initReactI18next).init({ resources: { "en-US": { translation: en }, "zh-CN": { translation: zh } }, lng: "en-US", fallbackLng: "en-US", interpolation: { escapeValue: false } });
export default i18n;
