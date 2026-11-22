import { createContext, useContext, useState, type Dispatch, type SetStateAction } from "react";
export const SearchKeywordContext = createContext<
  [string, Dispatch<SetStateAction<string>>] | null
>(null);
export function useSearchKeyword() {
  const shared = useContext(SearchKeywordContext);
  const local = useState("");
  return shared ?? local;
}
