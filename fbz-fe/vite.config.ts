import { execFileSync } from "node:child_process";
/// <reference types="vitest/config" />

import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import vueJsx from "@vitejs/plugin-vue-jsx";
import { loadEnv } from "vite";
import { defineConfig } from "vite-plus";
import UnoCSS from "unocss/vite";
import AutoImport from "unplugin-auto-import/vite";
import Components from "unplugin-vue-components/vite";

const DEV_API_TARGET_FALLBACK = "http://127.0.0.1:8080";
const EMBY_COMPAT_PROXY_PREFIXES = [
  "/api",
  "/emby",
  "/Albums",
  "/Artists",
  "/Books",
  "/Audio",
  "/Collections",
  "/DisplayPreferences",
  "/Environment",
  "/Genres",
  "/Images",
  "/Items",
  "/Library",
  "/MusicGenres",
  "/Persons",
  "/Playlists",
  "/Search",
  "/Sessions",
  "/Shows",
  "/SyncPlay",
  "/System",
  "/UserData",
  "/Users",
  "/Videos",
  "/embywebsocket",
] as const;

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");
  const devApiTarget = env.VITE_DEV_API_TARGET ?? env.FBZ_DEV_API_TARGET ?? DEV_API_TARGET_FALLBACK;

  return {
    plugins: [
      {
        name: "fbz-webplayer-assets",
        buildStart() {
          execFileSync(process.execPath, ["scripts/build-webplayer.mjs"], {
            stdio: "inherit",
            windowsHide: true,
          });
        },
      },
      tailwindcss(),
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
    server: {
      proxy: Object.fromEntries(
        EMBY_COMPAT_PROXY_PREFIXES.map((prefix) => [
          prefix,
          {
            target: devApiTarget,
            changeOrigin: true,
            // SyncPlay / 远程控制走 /embywebsocket。
            ws: prefix === "/embywebsocket",
          },
        ]),
      ),
    },
    build: {
      rollupOptions: {
        onLog(level, log, handler) {
          if (log.code === "INVALID_ANNOTATION" && log.loc?.file?.includes("@vueuse/core")) {
            return;
          }

          handler(level, log);
        },
      },
    },
    staged: {
      "*": "vp check --fix",
    },
    test: {
      include: ["src/**/*.{test,spec}.{js,ts,jsx,tsx}"],
      exclude: ["**/node_modules/**", "**/dist/**", "**/dist-ssr/**", "**/.tmp/**"],
      // Node ≥25 自带 webstorage 全局（无 --localstorage-file 时为 undefined），会遮蔽 jsdom 的 localStorage。
      execArgv: ["--no-experimental-webstorage"],
    },
    fmt: {
      ignorePatterns: [
        "src/auto-imports.d.ts",
        "src/components.d.ts",
        "vendor/**",
        "public/webplayer/**",
      ],
    },
    lint: {
      ignorePatterns: ["vendor/**", "public/webplayer/**"],
      options: { typeAware: true, typeCheck: true },
    },
  };
});
