import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

const configuredSite = process.env.ASTRO_SITE ?? process.env.CI_PAGES_URL;
const configuredBase = process.env.ASTRO_BASE;

function normalizeBase(base) {
  if (!base || base === "/") {
    return "/";
  }

  const withLeadingSlash = base.startsWith("/") ? base : `/${base}`;
  return withLeadingSlash.endsWith("/")
    ? withLeadingSlash
    : `${withLeadingSlash}/`;
}

function baseFromPagesUrl(pagesUrl) {
  if (!pagesUrl) {
    return "/";
  }

  try {
    return normalizeBase(new URL(pagesUrl).pathname);
  } catch {
    return "/";
  }
}

const base = configuredBase
  ? normalizeBase(configuredBase)
  : baseFromPagesUrl(process.env.CI_PAGES_URL);

export default defineConfig({
  site: configuredSite ?? "http://localhost:4321",
  base,
  integrations: [
    starlight({
      title: "N-Queens Problem",
    }),
  ],
});
