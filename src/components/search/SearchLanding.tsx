import type { ReactNode } from "react";
import { isWindows } from "@/lib/platform";
/** Anchor the search row independently of the mode switch and optional tools. */
export default function SearchLanding({
  title,
  modeSwitch,
  children,
  tools,
}: {
  title: string;
  modeSwitch?: ReactNode;
  children: ReactNode;
  tools?: ReactNode;
}) {
  return (
    <div className="relative min-h-0 flex-1">
      <div
        data-tauri-drag-region={isWindows ? undefined : true}
        className="search-top-spacer absolute inset-x-0 top-0 h-16"
      />
      <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 px-8">
        <div data-usage-search className="relative mx-auto w-full max-w-xl">
          <div className="absolute bottom-full mb-8 flex w-full flex-col items-center gap-8">
            {modeSwitch}
            <h1 className="text-center text-2xl font-semibold">{title}</h1>
          </div>
          {children}
          {tools && (
            <div className="absolute inset-x-0 top-full mt-8 flex flex-col items-center gap-8">
              {tools}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
