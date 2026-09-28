// The one ordered list of docs pages. The sidebar, previous/next links,
// and search all read it, so a page added here shows up in all three.

export interface NavPage {
  title: string;
  href: string;
  description: string;
}

export interface NavSection {
  title: string;
  pages: NavPage[];
}

export const nav: NavSection[] = [
  {
    title: "Start here",
    pages: [
      {
        title: "Getting started",
        href: "/getting-started/",
        description: "Install the CLI, create an app, run it, and make your first SDK call."
      },
      {
        title: "How Chain works",
        href: "/architecture/",
        description: "The layers a call passes through, and what each one is responsible for."
      },
      {
        title: "Core concepts",
        href: "/concepts/",
        description: "Capabilities, normalized errors, capability detection, and status levels."
      }
    ]
  },
  {
    title: "Capabilities",
    pages: [
      {
        title: "Overview",
        href: "/capabilities/",
        description: "Every capability, what it's for, and where it runs today."
      },
      {
        title: "Platform",
        href: "/capabilities/platform/",
        description: "desktop.platform — the OS, CPU architecture, and runtime version."
      },
      {
        title: "Storage",
        href: "/capabilities/storage/",
        description: "desktop.storage — an app's own SQLite database: migrate, query, execute."
      },
      {
        title: "Files",
        href: "/capabilities/files/",
        description: "desktop.files — store bytes under an opaque reference and show them in the UI."
      },
      {
        title: "Http",
        href: "/capabilities/http/",
        description: "desktop.http — fetch a page from any origin, without CORS getting in the way."
      },
      {
        title: "Agent server",
        href: "/capabilities/agent-server/",
        description: "desktop.agentServer — a localhost HTTP endpoint that AI agents can call into."
      },
      {
        title: "Process runner",
        href: "/capabilities/process-runner/",
        description: "desktop.processRunner — run a program and stream its output as it prints."
      }
    ]
  },
  {
    title: "CLI",
    pages: [
      {
        title: "Overview",
        href: "/cli/",
        description: "Every chain command at a glance."
      },
      {
        title: "chain init",
        href: "/cli/init/",
        description: "Create a new Chain app with React, routing, Tailwind, and the SDK wired up."
      },
      {
        title: "chain dev",
        href: "/cli/dev/",
        description: "Run the app with hot reload and condensed, readable output."
      },
      {
        title: "chain build",
        href: "/cli/build/",
        description: "Produce a release build and installer for the current OS."
      },
      {
        title: "chain update",
        href: "/cli/update/",
        description: "Pull framework updates into an existing app without losing your edits."
      },
      {
        title: "chain inspect",
        href: "/cli/inspect/",
        description: "Drive a running dev window from the terminal: eval, click, read, screenshot."
      },
      {
        title: "chain migration",
        href: "/cli/migration/",
        description: "Generate migrations from your schema classes, like dotnet ef migrations."
      },
      {
        title: "chain database",
        href: "/cli/database/",
        description: "Apply or revert migrations, and adopt hand-written ones."
      },
      {
        title: "chain clean",
        href: "/cli/clean/",
        description: "Free the disk space used by native build caches."
      },
      {
        title: "chain doctor",
        href: "/cli/doctor/",
        description: "Check for, and optionally install, the Rust toolchain."
      }
    ]
  },
  {
    title: "Project",
    pages: [
      {
        title: "Contributing",
        href: "/contributing/",
        description: "How new capabilities get proposed and built."
      }
    ]
  }
];

export const allPages: NavPage[] = nav.flatMap((section) => section.pages);
