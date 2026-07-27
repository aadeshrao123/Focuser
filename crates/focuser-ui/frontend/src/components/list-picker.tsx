import { useId } from "react";
import type { BlockList } from "@/bindings";
import { Select } from "@/components/ui/select";

/** Choose which block list a page is editing. */
export function ListPicker({
  lists,
  value,
  onChange,
}: {
  lists: BlockList[];
  value: string;
  onChange: (id: string) => void;
}) {
  const id = useId();
  if (lists.length === 0) return null;

  return (
    <div className="flex items-center gap-2 text-muted-foreground text-sm">
      <label htmlFor={id}>Block list</label>
      <Select
        id={id}
        value={value}
        onValueChange={onChange}
        options={lists.map((l) => ({ value: l.id, label: l.name }))}
        className="min-w-48"
      />
    </div>
  );
}

/** Keep a selected id valid as lists are added or removed. */
export function resolveSelected(lists: BlockList[], selected: string): string {
  if (lists.some((l) => l.id === selected)) return selected;
  return lists[0]?.id ?? "";
}
