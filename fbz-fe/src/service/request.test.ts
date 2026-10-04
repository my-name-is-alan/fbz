// @vitest-environment jsdom
import { beforeEach, expect, it } from "vitest";
import { getAccessToken, setAccessToken } from "./request.ts";
beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});
it("migrates a temporary legacy session without persisting its token", () => {
  sessionStorage.setItem(
    "fbz_session",
    JSON.stringify({ token: "temporary", userId: "user", username: "name" }),
  );
  expect(getAccessToken()).toBe("temporary");
  expect(localStorage.getItem("fbz_access_token")).toBeNull();
  expect(sessionStorage.getItem("fbz_access_token")).toBe("temporary");
  expect(localStorage.getItem("fbz_auth_user_id")).toBe("user");
});
it("logout cannot resurrect a legacy session", () => {
  localStorage.setItem("fbz_session", JSON.stringify({ token: "old", userId: "user" }));
  setAccessToken(null);
  expect(getAccessToken()).toBeNull();
});
it("uses the canonical token when an older legacy session still exists", () => {
  setAccessToken("current");
  localStorage.setItem("fbz_session", JSON.stringify({ token: "old", userId: "user" }));
  expect(getAccessToken()).toBe("current");
});
