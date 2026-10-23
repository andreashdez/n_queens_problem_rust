import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

function normalizeBase(base) {
  if (!base || base === "/") {
    return "/";
  }

  const withLeadingSlash = base.startsWith("/") ? base : `/${base}`;
  return withLeadingSlash.endsWith("/")
    ? withLeadingSlash
    : `${withLeadingSlash}/`;
}

export default defineConfig({
  site: process.env.ASTRO_SITE ?? "http://localhost:4321",
  base: normalizeBase(process.env.ASTRO_BASE),
  integrations: [
    starlight({
      title: "N-Queens in Rust",
      customCss: ["./src/styles/docs.css"],
      social: [
        {
          icon: "github",
          label: "GitHub repository",
          href: "https://github.com/andreashdez/n_queens_problem_rust",
        },
      ],
      editLink: {
        baseUrl: "https://github.com/andreashdez/n_queens_problem_rust/edit/main/website/",
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
