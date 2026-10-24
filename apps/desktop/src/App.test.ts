import { mount } from "@vue/test-utils";
import App from "./App.vue";

describe("App", () => {
  it("explains that confirmation fails closed during the prototype", () => {
    const wrapper = mount(App);
    expect(wrapper.get("h1").text()).toBe("GitGuard");
    expect(wrapper.text()).toContain("需要确认的推送会被拒绝");
  });
});
