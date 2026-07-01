import type { Router } from "vue-router";

import { getAccessToken } from "@/service/request.ts";

// 无需登录即可访问的路由名
const PUBLIC_ROUTE_NAMES = new Set<string>(["login", "not-found"]);

export function setupRouterGuard(router: Router) {
  router.beforeEach((to) => {
    const authenticated = getAccessToken() != null;

    // 已登录访问登录页 → 跳到 redirect 指向页或首页
    if (authenticated && to.name === "login") {
      const redirect = to.query.redirect;
      return { path: typeof redirect === "string" ? redirect : "/" };
    }

    // 未登录访问受保护页 → 重定向到登录页，记住目标
    if (!authenticated && !PUBLIC_ROUTE_NAMES.has(String(to.name))) {
      return { name: "login", query: { redirect: to.fullPath } };
    }

    return true;
  });
}
