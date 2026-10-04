// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import PluginAdminFrame from "./PluginAdminFrame.vue";
const calls = vi.hoisted(() => ({ get: vi.fn(), post: vi.fn() }));
vi.mock("@/service/request.ts", () => ({ request: calls }));
beforeEach(() => {
  calls.get.mockReset();
  calls.post.mockReset();
  calls.get.mockResolvedValue({
    data: { html: "<h1>Installed plugin</h1>", actions: ["status"], storageProvider: true },
  });
  calls.post.mockResolvedValue({ data: { ready: true } });
});
it("renders approved package HTML in an isolated frame and limits actions", async () => {
  const wrapper = mount(PluginAdminFrame, {
    props: { pluginId: "org.fbz.guangya", pagePath: "/admin/plugins/org.fbz.guangya/cloud" },
    attachTo: document.body,
  });
  await flushPromises();
  const frame = wrapper.find("iframe");
  expect(frame.exists()).toBe(true);
  expect(frame.attributes("sandbox")).toBe("allow-scripts");
  expect(frame.attributes("srcdoc")).toContain("connect-src 'none'");
  expect(frame.attributes("srcdoc")).toContain("Installed plugin");
  const source = (frame.element as HTMLIFrameElement).contentWindow;
  window.dispatchEvent(
    new MessageEvent("message", {
      origin: "null",
      source,
      data: { type: "fbz-plugin-action", id: "1", action: "unknown", payload: {} },
    }),
  );
  window.dispatchEvent(
    new MessageEvent("message", {
      origin: "https://other.invalid",
      source,
      data: { type: "fbz-plugin-action", id: "2", action: "status", payload: {} },
    }),
  );
  window.dispatchEvent(
    new MessageEvent("message", {
      origin: "null",
      source: window,
      data: { type: "fbz-plugin-action", id: "3", action: "status", payload: {} },
    }),
  );
  expect(calls.post).not.toHaveBeenCalled();
  window.dispatchEvent(
    new MessageEvent("message", {
      origin: "null",
      source,
      data: { type: "fbz-plugin-action", id: "4", action: "status", payload: {} },
    }),
  );
  await flushPromises();
  expect(calls.post).toHaveBeenCalledTimes(1);
  expect(calls.post).toHaveBeenCalledWith("/admin/plugins/org.fbz.guangya/ui/actions/status", {
    data: {},
  });
  wrapper.unmount();
});
