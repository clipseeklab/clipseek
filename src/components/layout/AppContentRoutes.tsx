import { Navigate, Route, Routes, useLocation, useParams, type Location } from "react-router-dom";
import SearchPage from "@/pages/SearchPage";
import AssetsPage, { TagAssetsRoute } from "@/pages/AssetsPage";
import TagsPage from "@/pages/TagsPage";
import UncategorizedPage from "@/pages/UncategorizedPage";
import FoldersPage from "@/pages/FoldersPage";
import TrashPage from "@/pages/TrashPage";
export default function AppContentRoutes() {
  const location = useLocation();
  const background = (
    location.state as {
      backgroundLocation?: Location;
    } | null
  )?.backgroundLocation;
  const overlay = location.pathname === "/search" && background;
  return (
    <>
      <div
        className="h-full"
        style={{ visibility: overlay ? "hidden" : undefined }}
        aria-hidden={!!overlay}
        inert={!!overlay}
      >
        <Routes location={overlay ? background : location}>
          <Route path="/" element={<Navigate to="/assets" replace />} />
          <Route path="/search" element={<SearchPage />} />
          <Route path="/assets" element={<AssetsPage />} />
          <Route path="/tags" element={<TagsPage />} />
          <Route path="/tag/:tagName" element={<TagAssetsPage />} />
          <Route path="/uncategorized" element={<UncategorizedPage />} />
          <Route path="/folders" element={<FoldersPage />} />
          <Route path="/folders/:folderId" element={<FoldersPage />} />
          <Route path="/trash" element={<TrashPage />} />
        </Routes>
      </div>
      {overlay && (
        <div className="absolute inset-0 bg-background">
          <SearchPage />
        </div>
      )}
    </>
  );
}
function TagAssetsPage() {
  const { tagName } = useParams<{
    tagName: string;
  }>();
  return tagName ? <TagAssetsRoute tagName={tagName} /> : null;
}
