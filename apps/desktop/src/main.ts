import { createApp } from "vue";
import App from "./App.vue";
import { i18n } from "./i18n";
import { installGlobalErrorReporting } from "./utils/reportError";
import "./style.css";

installGlobalErrorReporting();
createApp(App).use(i18n).mount("#app");
