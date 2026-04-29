(() => {
	const GH_BASE =
		"https://github.com/nobodywho-ooo/nobodywho/blob/main/docs/markdown/";

	const menuClass =
		"absolute right-0 hidden flex-col gap-1 p-2 rounded-md border " +
		"bg-popover text-popover-foreground shadow-lg text-sm";

	const itemClass =
		"flex items-center gap-2 whitespace-nowrap rounded-md px-2 py-1 " +
		"hover:bg-accent hover:text-accent-foreground cursor-pointer";

	// Icon SVG paths. Brand icons are from Simple Icons (CC0). The markdown
	// icon is the official Markdown mark.
	const ICONS = {
		chatgpt:
			'<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z"/></svg>',
		claude:
			'<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z"/></svg>',
		github:
			'<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>',
		markdown:
			'<svg viewBox="0 0 208 128" width="16" height="16" fill="none" stroke="currentColor" stroke-width="10" aria-hidden="true"><rect x="5" y="5" width="198" height="118" rx="15" ry="15"/><path d="M30 98V30h20l20 25 20-25h20v68H90V59L70 84 50 59v39zM155 98V30h20v34h20l-30 34-30-34h20v34" fill="currentColor" stroke="none"/></svg>',
	};

	// mkdocs-llms-source emits .md siblings mirroring the source layout, but
	// MkDocs pretty-URLs collapse two cases onto the same URL shape:
	//   /python/      -> /python/index.md   (source: python/index.md)
	//   /llm-basics/  -> /llm-basics.md     (source: llm-basics.md)
	// We can't tell from the URL alone, so we probe both candidates with HEAD.
	function mdCandidates() {
		const p = location.pathname;
		if (p.endsWith(".html")) {
			return [p.slice(0, -5) + ".md"];
		}
		if (p.endsWith("/")) {
			const stripped = p.replace(/\/$/, "");
			const out = [p + "index.md"];
			if (stripped) out.push(stripped + ".md");
			return out;
		}
		return [p + ".md"];
	}

	async function resolveMdPath() {
		const cands = mdCandidates();
		for (const c of cands) {
			try {
				const r = await fetch(c, { method: "HEAD" });
				if (r.ok) return c;
			} catch {}
		}
		return cands[0];
	}

	function buildLink(href, label, iconKey) {
		const a = document.createElement("a");
		a.href = href;
		a.target = "_blank";
		a.rel = "noreferrer noopener";
		a.className = itemClass;
		a.setAttribute("role", "menuitem");
		if (iconKey && ICONS[iconKey]) {
			const span = document.createElement("span");
			span.className = "shrink-0 inline-flex items-center";
			span.setAttribute("aria-hidden", "true");
			span.innerHTML = ICONS[iconKey];
			a.appendChild(span);
		}
		const text = document.createElement("span");
		text.textContent = label;
		a.appendChild(text);
		return a;
	}

	// Replace the theme's inline onclick with our own copy logic so we can
	// give the user feedback (icon swap + label change) once the clipboard
	// write actually completes.
	function enhanceCopyButton(btn, copyIcon, mdUrl) {
		const label = btn.querySelector("span");
		if (!label) return;

		const originalIconHTML = copyIcon.outerHTML;
		const originalLabel = label.textContent;
		// Build a check icon that inherits the copy icon's stroke/size attrs,
		// just with a single check-shaped path and renamed class.
		const checkIcon = copyIcon.cloneNode(false);
		checkIcon.classList.remove("tabler-icon-copy");
		checkIcon.classList.add("tabler-icon-check");
		checkIcon.innerHTML = '<path d="M5 12l5 5L20 7"></path>';
		const checkIconHTML = checkIcon.outerHTML;

		btn.removeAttribute("onclick");
		btn.addEventListener("click", async () => {
			// Lock the button's current width so it doesn't reflow when the
			// shorter "Copied!" / "Copy failed" label replaces "Copy Page".
			const lockedWidth = btn.getBoundingClientRect().width;
			btn.style.minWidth = lockedWidth + "px";
			try {
				const res = await fetch(mdUrl);
				if (!res.ok) throw new Error("HTTP " + res.status);
				const text = await res.text();
				await navigator.clipboard.writeText(text);
				btn.querySelector("svg").outerHTML = checkIconHTML;
				label.textContent = "Copied!";
			} catch {
				label.textContent = "Copy failed";
			}
			setTimeout(() => {
				const svg = btn.querySelector("svg");
				if (svg) svg.outerHTML = originalIconHTML;
				label.textContent = originalLabel;
				btn.style.minWidth = "";
			}, 1500);
		});
	}

	async function init() {
		const header = document.getElementById("page-header");
		if (!header) return;
		if (document.getElementById("open-dropdown")) return;

		// Anchor everything to the theme's built-in Copy Page button: its parent
		// is the button group we want to inject into, and it's the element we
		// enhance for clipboard feedback. If it's missing, the theme structure
		// has changed and we bail rather than guess.
		const copyIcon = header.querySelector("svg.tabler-icon-copy");
		const copyBtn = copyIcon ? copyIcon.closest("button") : null;
		const group = copyBtn ? copyBtn.parentElement : null;
		if (!group) return;

		const mdPath = await resolveMdPath();
		const mdUrl = location.origin + mdPath;
		const srcPath = mdPath.replace(/^\/+/, "");
		const githubUrl = GH_BASE + srcPath;
		const q = encodeURIComponent(
			"Read " + mdUrl + ", I want to ask questions about it.",
		);

		enhanceCopyButton(copyBtn, copyIcon, mdUrl);

		const wrap = document.createElement("div");
		wrap.id = "open-dropdown";
		wrap.className = "relative";

		const menuId = "open-dropdown-menu";
		// Clone the theme's Copy Page button for free styling parity, then
		// swap out its icon, label, and behavior for our dropdown trigger.
		const trigger = copyBtn.cloneNode(true);
		trigger.removeAttribute("onclick");
		trigger.type = "button";
		trigger.setAttribute("aria-haspopup", "menu");
		trigger.setAttribute("aria-expanded", "false");
		trigger.setAttribute("aria-controls", menuId);
		const triggerSvg = trigger.querySelector("svg");
		if (triggerSvg) {
			triggerSvg.classList.remove("tabler-icon-copy");
			triggerSvg.classList.add("tabler-icon-external-link");
			triggerSvg.innerHTML =
				'<path d="M12 6H6a2 2 0 0 0 -2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-6"></path>' +
				'<path d="M11 13l9 -9"></path>' +
				'<path d="M15 4h5v5"></path>';
		}
		const triggerLabel = trigger.querySelector("span");
		if (triggerLabel) triggerLabel.textContent = "Open in \u25BE";

		const menu = document.createElement("div");
		menu.id = menuId;
		menu.className = menuClass;
		menu.setAttribute("role", "menu");
		menu.style.top = "100%";
		menu.style.marginTop = "4px";
		menu.style.minWidth = "200px";
		menu.style.zIndex = "50";

		menu.appendChild(
			buildLink(
				"https://chatgpt.com/?hints=search&q=" + q,
				"ChatGPT",
				"chatgpt",
			),
		);
		menu.appendChild(
			buildLink("https://claude.ai/new?q=" + q, "Claude", "claude"),
		);
		menu.appendChild(buildLink(mdUrl, "View raw .md", "markdown"));
		menu.appendChild(buildLink(githubUrl, "View on GitHub", "github"));

		const setOpen = (open) => {
			menu.classList.toggle("hidden", !open);
			menu.classList.toggle("flex", open);
			trigger.setAttribute("aria-expanded", open ? "true" : "false");
		};

		trigger.addEventListener("click", (e) => {
			e.stopPropagation();
			setOpen(menu.classList.contains("hidden"));
		});

		document.addEventListener("click", (e) => {
			if (!wrap.contains(e.target)) setOpen(false);
		});

		document.addEventListener("keydown", (e) => {
			if (e.key === "Escape") setOpen(false);
		});

		wrap.appendChild(trigger);
		wrap.appendChild(menu);

		// Place the dropdown right after the Copy Page button.
		if (copyBtn.nextSibling) {
			group.insertBefore(wrap, copyBtn.nextSibling);
		} else {
			group.appendChild(wrap);
		}
	}

	if (document.readyState === "loading") {
		document.addEventListener("DOMContentLoaded", init);
	} else {
		init();
	}
})();
