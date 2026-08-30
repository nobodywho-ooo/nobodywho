(() => {
  const bindingNames = {
    kotlin: "Kotlin",
    python: "Python",
    swift: "Swift",
    "react-native": "React Native",
    flutter: "Flutter",
    godot: "Godot",
  };

  function routeFor({binding, version, page}) {
    const versionPath = version === "latest" ? "" : `${version}/`;
    return `/${binding}/${versionPath}${page}`;
  }

  function pageContext({binding, versions}) {
    const segments = window.location.pathname.split("/").filter(Boolean);
    const tail = segments.slice(1);
    const knownVersions = ["main", ...versions.older];
    const version = knownVersions.includes(tail[0]) ? tail.shift() : "latest";
    const page = tail.join("/");
    return {binding, page, version};
  }

  function addVersionNotice({context, versions}) {
    if (context.version === "latest") return;

    const header = document.querySelector("main.content #title-block-header");
    if (!header) return;

    const notice = document.createElement("div");
    notice.className = "callout callout-style-simple callout-note version-notice";

    const currentUrl = routeFor({
      binding: context.binding,
      version: "latest",
      page: context.page,
    });
    const currentLink = document.createElement("a");
    currentLink.href = currentUrl;
    currentLink.textContent = `Read version ${versions.latest}`;

    const paragraph = document.createElement("p");
    if (context.version === "main") {
      paragraph.append(
        "These docs describe the unreleased main branch. ",
        currentLink,
        " for the latest release.",
      );
    } else {
      paragraph.append(
        `These docs cover version ${context.version}, which is no longer maintained. `,
        currentLink,
        " for current guidance.",
      );
    }

    const body = document.createElement("div");
    body.className = "callout-body d-flex";
    const iconContainer = document.createElement("div");
    iconContainer.className = "callout-icon-container";
    const icon = document.createElement("i");
    icon.className = "callout-icon";
    const content = document.createElement("div");
    content.className = "callout-body-container";

    iconContainer.append(icon);
    content.append(paragraph);
    body.append(iconContainer, content);
    notice.append(body);
    header.insertAdjacentElement("afterend", notice);
  }

  function addVersionControl({context, versions}) {
    const sidebarMenu = document.querySelector(".sidebar-menu-container");
    if (!sidebarMenu) return;

    const control = document.createElement("div");
    control.className = "version-control px-2 mb-3";

    const label = document.createElement("label");
    label.className = "form-label small mb-1";
    label.htmlFor = "docs-version";
    label.textContent = `${bindingNames[context.binding]} version`;

    const select = document.createElement("select");
    select.className = "form-select form-select-sm";
    select.id = "docs-version";
    select.setAttribute("aria-label", `${bindingNames[context.binding]} documentation version`);

    const options = [
      {label: "main, unreleased", value: "main"},
      {label: `${versions.latest}, latest`, value: "latest"},
      ...versions.older.map((version) => ({label: `${version}, older`, value: version})),
    ];

    for (const optionData of options) {
      const option = document.createElement("option");
      option.value = optionData.value;
      option.textContent = optionData.label;
      option.selected = optionData.value === context.version;
      select.append(option);
    }

    select.addEventListener("change", async () => {
      const target = routeFor({
        binding: context.binding,
        version: select.value,
        page: context.page,
      });
      const fallback = routeFor({
        binding: context.binding,
        version: select.value,
        page: "",
      });

      try {
        const response = await fetch(target, {method: "HEAD"});
        window.location.assign(response.ok ? target : fallback);
      } catch {
        window.location.assign(fallback);
      }
    });

    control.append(label, select);
    sidebarMenu.prepend(control);
  }

  async function initialize() {
    document.querySelector(".navbar")?.setAttribute("aria-label", "Primary navigation");
    document.querySelector("#quarto-sidebar")?.setAttribute("aria-label", "Documentation");
    document.querySelector("#TOC")?.setAttribute("aria-label", "On this page");
    document.querySelector(".quarto-secondary-nav")?.setAttribute("aria-label", "Mobile navigation");
    document.querySelector(".page-navigation")?.setAttribute("aria-label", "Page navigation");

    const binding = window.location.pathname.split("/").filter(Boolean)[0];
    if (!bindingNames[binding]) return;

    const response = await fetch("/versions.json");
    if (!response.ok) return;

    const allVersions = await response.json();
    const versions = allVersions[binding];
    const context = pageContext({binding, versions});
    addVersionControl({context, versions});
    addVersionNotice({context, versions});
  }

  initialize().catch((error) => {
    console.error("Unable to initialize the documentation version selector.", error);
  });
})();
