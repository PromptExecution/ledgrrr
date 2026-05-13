(function () {
    const ORIGINAL_CLASS = "rhai-diagram-original";
    const MERMAID_CDN = "https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js";
    const STORAGE_KEY = "rhai-live-view-mode";

    function escapeHtml(raw) {
        return String(raw)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;");
    }

    async function ensureMermaid() {
        if (window.mermaid) {
            return window.mermaid;
        }

        if (!window.__rhaiLiveMermaidLoadPromise) {
            window.__rhaiLiveMermaidLoadPromise = new Promise(function (resolve, reject) {
                const script = document.createElement("script");
                script.src = MERMAID_CDN;
                script.async = true;
                script.onload = function () {
                    if (window.mermaid) {
                        resolve(window.mermaid);
                    } else {
                        reject(new Error("Mermaid script loaded but window.mermaid is still undefined."));
                    }
                };
                script.onerror = function () {
                    reject(
                        new Error(
                            "Mermaid failed to load from CDN. Check network access or include Mermaid in mdBook assets."
                        )
                    );
                };
                document.head.appendChild(script);
            });
        }

        return window.__rhaiLiveMermaidLoadPromise;
    }

    async function renderMermaid(target, mermaidSource) {
        const mermaid = await ensureMermaid();
        if (!window.__rhaiLiveMermaidInitialized) {
            mermaid.initialize({ startOnLoad: false });
            window.__rhaiLiveMermaidInitialized = true;
        }

        const renderId = `rhai-live-${Math.random().toString(36).slice(2)}`;
        const rendered = await mermaid.render(renderId, mermaidSource);
        target.innerHTML = rendered.svg;
        if (typeof rendered.bindFunctions === "function") {
            rendered.bindFunctions(target);
        }
    }

    function diagnosticsHtml(diagnostics) {
        if (!diagnostics.length) {
            return "";
        }

        const lines = diagnostics.map(function (diag) {
            const cls =
                diag.kind === "error"
                    ? "rhai-diag-error"
                    : diag.kind === "warning"
                      ? "rhai-diag-warning"
                      : "rhai-diag-info";
            return `<li class="${cls}"><strong>L${diag.line}</strong> ${escapeHtml(diag.message)}<br><code>${escapeHtml(
                diag.source
            )}</code></li>`;
        });

        return `<div class="rhai-diag-panel"><div class="rhai-diag-title">Diagnostics</div><ul>${lines.join(
            ""
        )}</ul></div>`;
    }

    function renderFailureHtml(core, error, viewMode) {
        const failure = core.buildRenderFailure(error, viewMode);
        return `<div class="rhai-diagram-failure">
            <p class="rhai-diagram-error">${escapeHtml(failure.title)}</p>
            <p>${escapeHtml(failure.detail)}</p>
            <p class="rhai-diag-hint">${escapeHtml(failure.hint)}</p>
        </div>`;
    }

    function modeLabel(mode) {
        return mode === "mermaid-2d" ? "mermaid-2d" : "isometric-3d";
    }

    function selectedViewMode() {
        try {
            const stored = window.localStorage ? window.localStorage.getItem(STORAGE_KEY) : null;
            return stored === "mermaid-2d" ? "mermaid-2d" : "isometric-3d";
        } catch (_e) {
            return "isometric-3d";
        }
    }

    function persistViewMode(mode) {
        try {
            if (window.localStorage) {
                window.localStorage.setItem(STORAGE_KEY, mode);
            }
        } catch (_e) {
            // storage unavailable (private browsing, SecurityError) — ignore
        }
    }

    async function attachEditor(sourcePre) {
        const sourceCode = sourcePre && sourcePre.querySelector("code.language-rhai");
        const previewPre = sourcePre.nextElementSibling;
        if (!sourceCode || !previewPre || !previewPre.classList.contains("mermaid")) {
            return;
        }

        const core = window.RhaiLiveCore;
        if (!core) {
            return;
        }

        const shell = document.createElement("section");
        shell.className = "rhai-diagram-shell";

        const toolbar = document.createElement("div");
        toolbar.className = "rhai-diagram-toolbar";

        const left = document.createElement("div");
        left.className = "rhai-diagram-toolbar-left";

        const status = document.createElement("div");
        status.className = "rhai-diagram-status";
        status.textContent = "Live Rhai workflow editor";

        const switcher = document.createElement("label");
        switcher.className = "rhai-view-switch";
        switcher.innerHTML = `
            <span class="rhai-view-label">View</span>
            <span class="rhai-view-slider">
                <input class="rhai-view-input" type="checkbox" aria-label="Toggle between isometric and Mermaid views">
                <span class="rhai-view-track">
                    <span class="rhai-view-thumb"></span>
                    <span class="rhai-view-option" data-mode="isometric-3d">isometric-3d</span>
                    <span class="rhai-view-option" data-mode="mermaid-2d">mermaid-2d</span>
                </span>
            </span>
        `;

        left.appendChild(status);
        left.appendChild(switcher);

        const actions = document.createElement("div");
        actions.className = "rhai-diagram-actions";

        const regenerate = document.createElement("button");
        regenerate.type = "button";
        regenerate.className = "rhai-diagram-button";
        regenerate.textContent = "Regenerate";

        const reset = document.createElement("button");
        reset.type = "button";
        reset.className = "rhai-diagram-button";
        reset.textContent = "Reset";

        actions.appendChild(regenerate);
        actions.appendChild(reset);
        toolbar.appendChild(left);
        toolbar.appendChild(actions);

        const body = document.createElement("div");
        body.className = "rhai-diagram-body";

        const editor = document.createElement("textarea");
        editor.className = "rhai-diagram-editor";
        editor.spellcheck = false;
        editor.value = sourceCode.textContent || "";

        const preview = document.createElement("div");
        preview.className = "rhai-diagram-preview";

        const note = document.createElement("div");
        note.className = "rhai-diagram-note";
        note.textContent =
            "Edit the supported Rhai diagram DSL (`fn ... -> ...`, `if ... -> ...`, `match expr => Arm -> target`) and regenerate. The isometric view animates layout shifts when the graph changes.";

        const chat = document.createElement("section");
        chat.className = "rhai-model-chat";

        const chatTitle = document.createElement("div");
        chatTitle.className = "rhai-model-chat-title";
        chatTitle.textContent = "Rhai mutation prompt";

        const chatPrompt = document.createElement("textarea");
        chatPrompt.className = "rhai-model-chat-input";
        chatPrompt.spellcheck = false;
        chatPrompt.value =
            "Add a medium-confidence review path and keep workbook commit behind review or high confidence.";

        const chatActions = document.createElement("div");
        chatActions.className = "rhai-model-chat-actions";

        const preparePrompt = document.createElement("button");
        preparePrompt.type = "button";
        preparePrompt.className = "rhai-diagram-button";
        preparePrompt.textContent = "Prepare Model Prompt";

        const applyDraft = document.createElement("button");
        applyDraft.type = "button";
        applyDraft.className = "rhai-diagram-button";
        applyDraft.textContent = "Apply Example Draft";

        const chatOutput = document.createElement("pre");
        chatOutput.className = "rhai-model-chat-output";

        chatActions.appendChild(preparePrompt);
        chatActions.appendChild(applyDraft);
        chat.appendChild(chatTitle);
        chat.appendChild(chatPrompt);
        chat.appendChild(chatActions);
        chat.appendChild(chatOutput);

        body.appendChild(editor);
        body.appendChild(preview);
        shell.appendChild(toolbar);
        shell.appendChild(body);
        shell.appendChild(note);
        shell.appendChild(chat);

        sourcePre.classList.add(ORIGINAL_CLASS);
        previewPre.classList.add(ORIGINAL_CLASS);
        previewPre.insertAdjacentElement("afterend", shell);

        const originalSource = editor.value;
        const modeInput = switcher.querySelector(".rhai-view-input");
        const viewOptions = Array.from(switcher.querySelectorAll(".rhai-view-option"));
        let viewMode = selectedViewMode();
        let previousScene = null;

        function syncViewSwitch() {
            modeInput.checked = viewMode === "mermaid-2d";
            viewOptions.forEach(function (option) {
                option.classList.toggle("is-active", option.getAttribute("data-mode") === viewMode);
            });
        }

        async function update() {
            try {
                const result = core.parseRhaiDiagram(editor.value);
                const graph = result.graph;
                const diagnostics = result.diagnostics;

                if (!graph.order.length && !graph.edges.length) {
                    preview.innerHTML =
                        '<p class="rhai-diagram-error">No diagramable Rhai DSL lines found in this block.</p>' +
                        diagnosticsHtml(diagnostics);
                    status.textContent = "No parseable diagram nodes";
                    previousScene = null;
                    return;
                }

                const scene = core.buildVisualizationModel(graph);
                if (viewMode === "mermaid-2d") {
                    const mermaidSource = core.graphToMermaid(graph);
                    await renderMermaid(preview, mermaidSource);
                } else {
                    preview.innerHTML = core.sceneToIsometricSvg(scene, previousScene);
                }

                preview.insertAdjacentHTML("beforeend", diagnosticsHtml(diagnostics));
                status.textContent = `${modeLabel(viewMode)} · ${graph.nodes.size} nodes · ${graph.edges.length} edges`;
                previousScene = scene;
            } catch (error) {
                preview.innerHTML = renderFailureHtml(core, error, viewMode);
                status.textContent = `${modeLabel(viewMode)} failed`;
            }
        }

        modeInput.addEventListener("change", async function () {
            viewMode = modeInput.checked ? "mermaid-2d" : "isometric-3d";
            persistViewMode(viewMode);
            syncViewSwitch();
            await update();
        });

        regenerate.addEventListener("click", update);
        reset.addEventListener("click", async function () {
            editor.value = originalSource;
            await update();
        });

        preparePrompt.addEventListener("click", function () {
            chatOutput.textContent = core.buildRhaiMutationPrompt(editor.value, chatPrompt.value);
        });

        applyDraft.addEventListener("click", async function () {
            const draft = core.draftRhaiMutationFromChat(editor.value, chatPrompt.value);
            editor.value = draft.source;
            chatOutput.textContent = `Model example: ${draft.modelName}\n\n${draft.explanation}\n\nPrepared prompt:\n${draft.prompt}`;
            await update();
        });

        editor.addEventListener("keydown", async function (event) {
            if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                event.preventDefault();
                await update();
            }
        });

        syncViewSwitch();
        await update();
    }

    async function main() {
        const sourceBlocks = Array.from(document.querySelectorAll("pre"));
        for (const sourcePre of sourceBlocks) {
            const code = sourcePre.querySelector("code.language-rhai");
            if (!code) {
                continue;
            }

            const previewPre = sourcePre.nextElementSibling;
            if (!previewPre || !previewPre.classList.contains("mermaid")) {
                continue;
            }

            await attachEditor(sourcePre);
        }
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", function () {
            main().catch(function (error) {
                console.error("rhai-live:", error);
            });
        });
    } else {
        main().catch(function (error) {
            console.error("rhai-live:", error);
        });
    }
})();
