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
  // The docs site lives outside `src/`, which holds the Rust crate.
  srcDir: "./website",
  integrations: [
    starlight({
      title: "N-Queens in Rust",
      customCss: ["./website/styles/docs.css"],
      social: [
        {
          icon: "github",
          label: "GitHub repository",
          href: "https://github.com/andreashdez/n_queens_problem_rust",
        },
      ],
      editLink: {
        baseUrl: "https://github.com/andreashdez/n_queens_problem_rust/edit/main/",
      },
      sidebar: [
        { label: "Overview", slug: "" },
        {
          label: "Start here",
          items: [
            { label: "Getting started", slug: "getting-started" },
            { label: "GUI guide", slug: "gui" },
            { label: "CLI reference", slug: "cli" },
          ],
        },
        {
          label: "Understand and experiment",
          items: [
            { label: "How the algorithm works", slug: "algorithm" },
            { label: "Tuning the solver", slug: "tuning" },
            { label: "Benchmarks and profiling", slug: "benchmarks" },
          ],
        },
        {
          label: "Build and contribute",
          items: [
            { label: "Library usage", slug: "library" },
            { label: "Development", slug: "development" },
          ],
        },
      ],
    }),
  ],
});
