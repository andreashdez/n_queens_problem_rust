import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

const ciPagesUrl = process.env.CI_PAGES_URL;

const base = (() => {
  if (!ciPagesUrl) {
    return "/";
  }

  try {
    const pathname = new URL(ciPagesUrl).pathname;
    if (!pathname || pathname === "/") {
      return "/";
    }

    return pathname.endsWith("/") ? pathname : `${pathname}/`;
  } catch {
    return "/";
  }
})();

export default defineConfig({
  site: ciPagesUrl ?? "http://localhost:4321",
  base,
  integrations: [
    starlight({
      title: "N-Queens Problem",
    }),
  ],
});
