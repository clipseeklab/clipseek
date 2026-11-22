import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronsUpDown, Folder } from "lucide-react";
import FolderPicker, { FolderName } from "@/components/folders/FolderPicker";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
export default function FolderField({
  value,
  onChange,
  disabled = false,
}: {
  value: number | null;
  onChange: (value: number | null) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          ref={trigger}
          type="button"
          variant="outline"
          className="h-10 w-full justify-start font-normal"
          disabled={disabled}
          aria-label={t("organization.folder")}
        >
          <Folder className="size-4 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate text-left">
            {value === null ? t("assets.uncategorized") : <FolderName id={String(value)} />}
          </span>
          <ChevronsUpDown className="size-4 text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-80 max-w-[calc(100vw-2rem)] p-2"
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        <FolderPicker
          values={[String(value ?? "none")]}
          label={t("organization.folder")}
          disabled={disabled}
          onChange={(values) => {
            onChange(values[0] === "none" ? null : Number(values[0]));
            setOpen(false);
            trigger.current?.focus();
          }}
        />
      </PopoverContent>
    </Popover>
  );
}
