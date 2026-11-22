import { useState, type ComponentType, type ReactNode } from "react";
import { SearchKeywordContext } from "@/hooks/useSearchKeyword";
export default function SearchModePage({
  VisualSearch,
}: {
  VisualSearch: ComponentType<{
    modeSwitch: ReactNode;
    active: boolean;
    docked: boolean;
    onSearchStart: () => void;
  }>;
}) {
  const keyword = useState("");
  const [docked, setDocked] = useState(false);
  return (
    <SearchKeywordContext value={keyword}>
      <VisualSearch
        modeSwitch={null}
        active
        docked={docked}
        onSearchStart={() => setDocked(true)}
      />
    </SearchKeywordContext>
  );
}
