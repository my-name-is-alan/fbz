import "virtual:uno.css";
import "./styles/daisy.css";
import "./style.scss";

import { createApp } from "vue";

import App from "./App.vue";
import { setupPlugins } from "@/plugins/index.ts";

const app = createApp(App);

setupPlugins(app);
app.mount("#app");
