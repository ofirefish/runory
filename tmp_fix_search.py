from pathlib import Path

panel = Path("src/features/operations/DockerOnlineImagesPanel.tsx")
text = panel.read_text(encoding="utf-8")
old = """      <div className=\"flex flex-wrap items-center gap-2\">
        <Input
          value={draftQuery}
          onChange={(event) => setDraftQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === \"Enter\") void runSearch(draftQuery);
          }}
          placeholder={t(\"operations.dockerOnlineImages.searchPlaceholder\")}
          aria-label={t(\"operations.dockerOnlineImages.searchPlaceholder\")}
          className=\"min-w-56 flex-1\"
          disabled={disabled}
        />
        <Button type=\"button\" size=\"sm\" disabled={disabled || !draftQuery.trim()} onClick={() => void runSearch(draftQuery)}>
          <Search size={14} />
          {t(\"operations.dockerOnlineImages.search\")}
        </Button>
        <Button
          type=\"button\"
          variant=\"ghost\"
          size=\"icon\"
          className=\"h-8 w-8\"
          disabled={disabled || !activeQuery}
          aria-label={t(\"operations.dockerOnlineImages.refresh\")}
          onClick={() => void runSearch(activeQuery)}
        >
          <RefreshCw size={14} className={searchBusy ? \"animate-spin\" : undefined} />
        </Button>
      </div>"""

new = """      <form
        className=\"flex flex-wrap items-center gap-2\"
        onSubmit={(event) => {
          event.preventDefault();
          const form = new FormData(event.currentTarget);
          const query = String(form.get(\"query\") ?? draftQuery);
          setDraftQuery(query);
          void runSearch(query);
        }}
      >
        <Input
          name=\"query\"
          value={draftQuery}
          onChange={(event) => setDraftQuery(event.target.value)}
          placeholder={t(\"operations.dockerOnlineImages.searchPlaceholder\")}
          aria-label={t(\"operations.dockerOnlineImages.searchPlaceholder\")}
          className=\"min-w-56 flex-1\"
          disabled={disabled}
        />
        <Button type=\"submit\" size=\"sm\" disabled={disabled}>
          <Search size={14} />
          {t(\"operations.dockerOnlineImages.search\")}
        </Button>
        <Button
          type=\"button\"
          variant=\"ghost\"
          size=\"icon\"
          className=\"h-8 w-8\"
          disabled={disabled || !activeQuery}
          aria-label={t(\"operations.dockerOnlineImages.refresh\")}
          onClick={() => void runSearch(activeQuery)}
        >
          <RefreshCw size={14} className={searchBusy ? \"animate-spin\" : undefined} />
        </Button>
      </form>"""

if old not in text:
    raise SystemExit("toolbar not found")
panel.with_suffix(".tsx.new").write_text(text.replace(old, new, 1), encoding="utf-8")
print("panel ok")

test = Path("src/features/operations/DockerPanel.test.tsx")
t = test.read_text(encoding="utf-8")
t = t.replace('import { Simulate } from "react-dom/test-utils";\n', "")
old_t = """    await act(async () => {
      Simulate.change(input!, { target: { value: \"nginx\" } } as never);
    });

    const searchButton = Array.from(container.querySelectorAll(\"button\")).find((button) =>
      button.textContent?.includes(i18n.t(\"operations.dockerOnlineImages.search\")),
    ) as HTMLButtonElement | undefined;
    expect(searchButton).not.toBeUndefined();
    expect(searchButton!.disabled).toBe(false);
    await act(async () => { searchButton!.click(); });"""
new_t = """    const form = input!.closest(\"form\");
    expect(form).not.toBeNull();
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, \"value\")?.set;
      setter?.call(input!, \"nginx\");
      form!.requestSubmit();
    });"""
if old_t not in t:
    raise SystemExit("test block not found")
test.with_suffix(".tsx.new").write_text(t.replace(old_t, new_t, 1), encoding="utf-8")
print("test ok")
