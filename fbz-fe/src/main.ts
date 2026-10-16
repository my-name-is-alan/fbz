import "virtual:uno.css";
import "./styles/daisy.css";
import "./style.scss";
import "./styles/theme/hdhive.scss";

import { createApp } from "vue";

import App from "./App.vue";
import { setupPlugins } from "@/plugins/index.ts";

const app = createApp(App);

setupPlugins(app);
app.mount("#app");
