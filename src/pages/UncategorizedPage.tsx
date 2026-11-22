/** Uncategorized assets have folder_id IS NULL. */
import { AssetBrowserPage } from "./AssetsPage";
export default function UncategorizedPage() {
  return (
    <AssetBrowserPage filter={{ uncategorized: true }} emptyHint="assets" importFolder={null} />
  );
}
