import { useId, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { api } from "@/lib/api";
import { queryKeys } from "@/lib/queryKeys";
import { explainError } from "@/lib/errors";
import { collectTagNames, type TagInputValue } from "@/lib/tagInput";
import { useAppStore } from "@/stores/app";
import { Button } from "@/components/ui/button";
/** Controlled draft: callers can submit the final unconfirmed tag without losing text. */
export default function TagInput({
  value,
  onChange,
  autoFocus = false,
  disabled = false,
}: {
  value: TagInputValue;
  onChange: (value: TagInputValue) => void;
  autoFocus?: boolean;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const libraryId = useAppStore((state) => state.current?.id);
  const recent = useAppStore((state) => state.recentTagNames);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLInputElement>(null);
  const composing = useRef(false);
  const id = useId();
  const query = useQuery({
    queryKey: queryKeys.tags.library(libraryId),
    queryFn: api.tagList,
    enabled: !!libraryId,
    retry: false,
  });
  const tags = query.data ?? [];
  const keyword = value.input.trim().toLocaleLowerCase();
  const ranked = keyword
    ? tags.filter((tag) => tag.name.toLocaleLowerCase().includes(keyword))
    : [
        ...recent.flatMap((name) => tags.filter((tag) => tag.name === name)),
        ...[...tags].sort((a, b) => b.createdAt - a.createdAt),
      ];
  const suggestions = ranked
    .filter(
      (tag, index) =>
        !value.names.includes(tag.name) &&
        ranked.findIndex((other) => other.name === tag.name) === index,
    )
    .slice(0, 20);
  const add = (name?: string) => {
    onChange({
      names: collectTagNames({ names: value.names, input: name ?? value.input }),
      input: "",
    });
    setActive(-1);
    input.current?.focus();
  };
  const show = open && !disabled;
  return (
    <div
      className="relative min-w-0"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) {
          setOpen(false);
          setActive(-1);
        }
      }}
    >
      <div className="flex min-h-10 flex-wrap items-center gap-1.5 rounded-md border border-input bg-background px-2 py-1 focus-within:ring-2 focus-within:ring-ring/40">
        {value.names.map((name) => (
          <span
            key={name}
            className="inline-flex max-w-full items-center gap-1 rounded bg-secondary px-2 py-1 text-xs text-secondary-foreground"
          >
            <span className="truncate">{name}</span>
            <button
              type="button"
              disabled={disabled}
              aria-label={`${t("common.delete")} ${name}`}
              className="rounded-sm text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onClick={() =>
                onChange({ ...value, names: value.names.filter((tag) => tag !== name) })
              }
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        <input
          ref={input}
          autoFocus={autoFocus}
          disabled={disabled}
          role="combobox"
          aria-label={t("assets.tags")}
          aria-autocomplete="list"
          aria-expanded={show}
          aria-controls={show ? id : undefined}
          aria-activedescendant={
            show && active >= 0 && active < suggestions.length ? `${id}-${active}` : undefined
          }
          className="h-7 min-w-28 flex-1 bg-transparent px-1 text-sm outline-none placeholder:text-muted-foreground"
          placeholder={t("organization.tagPlaceholder")}
          value={value.input}
          onFocus={() => setOpen(true)}
          onCompositionStart={() => {
            composing.current = true;
          }}
          onCompositionEnd={() => {
            composing.current = false;
          }}
          onChange={(event) => {
            const text = event.target.value;
            if (
              /[,，\n]$/.test(text) &&
              !composing.current &&
              !(event.nativeEvent instanceof InputEvent && event.nativeEvent.isComposing)
            )
              onChange({ names: collectTagNames({ names: value.names, input: text }), input: "" });
            else onChange({ ...value, input: text });
            setActive(-1);
            setOpen(true);
          }}
          onKeyDown={(event) => {
            if (composing.current || event.nativeEvent.isComposing) return;
            if (event.key === "Enter") {
              event.preventDefault();
              add(show && active >= 0 ? suggestions[active]?.name : undefined);
            } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              setOpen(true);
              setActive((current) =>
                suggestions.length
                  ? current < 0
                    ? event.key === "ArrowDown"
                      ? 0
                      : suggestions.length - 1
                    : (current + (event.key === "ArrowDown" ? 1 : -1) + suggestions.length) %
                      suggestions.length
                  : -1,
              );
            } else if (event.key === "Escape" && open) {
              event.preventDefault();
              event.stopPropagation();
              setOpen(false);
              setActive(-1);
            } else if (event.key === "Backspace" && !value.input)
              onChange({ ...value, names: value.names.slice(0, -1) });
          }}
        />
      </div>
      {show && (
        <div className="mt-2 max-h-56 overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md">
          <div className="px-2 py-1 text-xs text-muted-foreground">
            {t(keyword ? "assets.tagSearchResults" : "organization.suggestedTags")}
          </div>
          <div role="listbox" id={id} aria-label={t("assets.tags")}>
            {suggestions.map((tag, index) => (
              <button
                id={`${id}-${index}`}
                key={tag.id}
                type="button"
                role="option"
                aria-label={tag.name}
                aria-selected={index === active}
                className="flex w-full items-center justify-between gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent aria-selected:bg-accent"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => add(tag.name)}
              >
                <span className="truncate">{tag.name}</span>
                <span className="text-xs text-muted-foreground">{tag.assetCount}</span>
              </button>
            ))}
          </div>
          {!suggestions.length && !query.isPending && !query.isError && (
            <p className="px-2 py-2 text-xs text-muted-foreground">
              {t(keyword ? "assets.tagNoResults" : "organization.noTags")}
            </p>
          )}
          {keyword &&
            !value.names.includes(value.input.trim()) &&
            !tags.some((tag) => tag.name === value.input.trim()) && (
              <button
                type="button"
                className="w-full rounded px-2 py-1.5 text-left text-sm hover:bg-accent"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => add()}
              >
                {t("organization.createTag", { name: value.input.trim() })}
              </button>
            )}
          {query.isError && (
            <p role="alert" className="p-2 text-xs text-destructive">
              {explainError(query.error)}
              <Button type="button" size="sm" variant="ghost" onClick={() => void query.refetch()}>
                {t("common.retry")}
              </Button>
            </p>
          )}
        </div>
      )}
    </div>
  );
}
