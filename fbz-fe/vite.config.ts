/// <reference types="vitest/config" />

import vue from "@vitejs/plugin-vue";
import vueJsx from "@vitejs/plugin-vue-jsx";
import { loadEnv } from "vite";
import { defineConfig } from "vite-plus";
import UnoCSS from "unocss/vite";
import AutoImport from "unplugin-auto-import/vite";
import Components from "unplugin-vue-components/vite";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");

  return {
    plugins: [
      UnoCSS(),
      vue(),
      vueJsx(),
      AutoImport({
        dts: "src/auto-imports.d.ts",
        imports: [
          "vue",
          "vue-router",
          "pinia",
          "@vueuse/core",
          {
            "lodash-es": ["debounce", "throttle", "cloneDeep"],
          },
        ],
      }),
      Components({
        dts: "src/components.d.ts",
        dirs: ["src/components"],
      }),
    ],
    resolve: {
      alias: {
        "@": "/src",
      },
    },
    css: {
      preprocessorOptions: {
        scss: {
          additionalData: '@use "@/styles/theme/tokens.scss" as *;',
        },
      },
    },
    define: {
      __APP_ENV__: JSON.stringify(env.APP_ENV ?? mode),
    },
    staged: {
      "*": "vp check --fix",
    },
    test: {
      include: ["src/**/*.{test,spec}.{js,ts,jsx,tsx}"],
      exclude: ["**/node_modules/**", "**/dist/**", "**/dist-ssr/**", "**/.tmp/**"],
    },
    fmt: {
      ignorePatterns: ["src/auto-imports.d.ts", "src/components.d.ts"],
    },
    lint: { options: { typeAware: true, typeCheck: true } },
  };
});
