import type { App } from "vue";

import { router } from "@/router/index.ts";
import { pinia } from "@/plugins/pinia.ts";

export function setupPlugins(app: App) {
  app.use(pinia);
  app.use(router);
}
