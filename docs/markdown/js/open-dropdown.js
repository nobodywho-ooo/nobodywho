(() => {
	const GH_BASE =
		"https://github.com/nobodywho-ooo/nobodywho/blob/main/docs/markdown/";

	const triggerClass =
		"inline-flex items-center justify-center whitespace-nowrap text-sm " +
		"font-medium transition-all hover:bg-accent hover:text-accent-foreground " +
		"rounded-md gap-2 px-3 h-8 shadow-none";

	const menuClass =
		"absolute right-0 hidden flex-col gap-1 p-2 rounded-md border " +
		"bg-popover text-popover-foreground shadow-lg text-sm";

	const itemClass =
		"flex items-center gap-2 whitespace-nowrap rounded-md px-2 py-1 " +
		"hover:bg-accent hover:text-accent-foreground";

	function buildLink(href, label) {
		const a = document.createElement("a");
		a.href = href;
		a.target = "_blank";
		a.rel = "noreferrer noopener";
		a.className = itemClass;
		a.textContent = label;
		return a;
	}

	function init() {
		const headerRight = document.querySelector("header .ml-auto");
		if (!headerRight) return;
		if (document.getElementById("open-dropdown")) return;

		const meta = document.querySelector('meta[name="docs-src-path"]');
		if (!meta) return;
		const srcPath = meta.content;
		const rawMdUrl = location.origin + "/" + srcPath;
		const githubUrl = GH_BASE + srcPath;
		const q = encodeURIComponent(
			"Read " + rawMdUrl + ", I want to ask questions about it.",
		);

		const wrap = document.createElement("div");
		wrap.id = "open-dropdown";
		wrap.className = "relative";

		const trigger = document.createElement("button");
		trigger.type = "button";
		trigger.className = triggerClass;
		trigger.textContent = "Open ▾";

		const menu = document.createElement("div");
		menu.className = menuClass;
		menu.style.top = "100%";
		menu.style.marginTop = "4px";
		menu.style.minWidth = "200px";
		menu.style.zIndex = "50";

		menu.appendChild(
			buildLink("https://chatgpt.com/?hints=search&q=" + q, "Open in ChatGPT"),
		);
		menu.appendChild(buildLink("https://claude.ai/new?q=" + q, "Open in Claude"));
		menu.appendChild(buildLink(githubUrl, "Open in GitHub"));

		const copyBtn = document.createElement("button");
		copyBtn.type = "button";
		copyBtn.className = itemClass + " text-left";
		copyBtn.textContent = "Copy markdown URL";
		copyBtn.addEventListener("click", () => {
			navigator.clipboard.writeText(rawMdUrl).then(() => {
				const original = copyBtn.textContent;
				copyBtn.textContent = "Copied!";
				setTimeout(() => {
					copyBtn.textContent = original;
				}, 1500);
			});
		});
		menu.appendChild(copyBtn);

		trigger.addEventListener("click", (e) => {
			e.stopPropagation();
			menu.classList.toggle("hidden");
			menu.classList.toggle("flex");
		});

		document.addEventListener("click", (e) => {
			if (!wrap.contains(e.target)) {
				menu.classList.add("hidden");
				menu.classList.remove("flex");
			}
		});

		document.addEventListener("keydown", (e) => {
			if (e.key === "Escape") {
				menu.classList.add("hidden");
				menu.classList.remove("flex");
			}
		});

		wrap.appendChild(trigger);
		wrap.appendChild(menu);

		// Insert before the GitHub-stars link so it sits left of stars/theme toggle.
		const starsLink = headerRight.querySelector('a[href*="github.com"]');
		if (starsLink) {
			headerRight.insertBefore(wrap, starsLink);
		} else {
			headerRight.insertBefore(wrap, headerRight.firstChild);
		}
	}

	if (document.readyState === "loading") {
		document.addEventListener("DOMContentLoaded", init);
	} else {
		init();
	}
})();
