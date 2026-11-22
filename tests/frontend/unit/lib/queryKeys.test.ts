import { QueryClient } from "@tanstack/react-query";
import { expect, it } from "vitest";
import { queryKeys } from "@/lib/queryKeys";

it("refreshes all folder views of one library without invalidating another library", async () => {
  const client = new QueryClient();
  const keys = [
    queryKeys.folders.page("a", "", 1, "name_asc"),
    queryKeys.folders.sidebar("a"),
    queryKeys.folders.detail("a", 1),
    queryKeys.folders.options("a", "photo"),
  ];
  const other = queryKeys.folders.detail("b", 1);
  for (const key of [...keys, other]) client.setQueryData(key, { name: key[1] });
  await client.invalidateQueries({ queryKey: queryKeys.folders.library("a") });
  for (const key of keys) expect(client.getQueryState(key)?.isInvalidated).toBe(true);
  expect(client.getQueryState(other)?.isInvalidated).toBe(false);
  expect(client.getQueryData(other)).toEqual({ name: "b" });
  client.clear();
});

it("keeps tag suggestions independent between libraries", async () => {
  const client = new QueryClient();
  const first = queryKeys.tags.library("a");
  const second = queryKeys.tags.library("b");
  client.setQueryData(first, [{ id: 1, name: "portrait" }]);
  client.setQueryData(second, [{ id: 1, name: "landscape" }]);
  await client.invalidateQueries({ queryKey: first });
  expect(client.getQueryData(second)).toEqual([{ id: 1, name: "landscape" }]);
  expect(client.getQueryState(second)?.isInvalidated).toBe(false);
  client.clear();
});

it("isolates processing, index, target and codec cache refreshes by library", async () => {
  const client = new QueryClient();
  const families = [
    {
      prefix: queryKeys.processing.library,
      keys: (id: string) => [
        queryKeys.processing.summary(id),
        queryKeys.processing.list(id, { status: "all", page: 2 }),
        queryKeys.processing.history(id, "asset:9"),
      ],
    },
    {
      prefix: queryKeys.indexTasks.library,
      keys: (id: string) => [queryKeys.indexTasks.asset(id, 9)],
    },
    {
      prefix: queryKeys.indexSummary.library,
      keys: (id: string) => [queryKeys.indexSummary.library(id)],
    },
    {
      prefix: queryKeys.targetOptions.library,
      keys: (id: string) => [
        queryKeys.targetOptions.search(id, "tags", "photo", [9]),
        queryKeys.targetOptions.search(id, "folders", "", []),
      ],
    },
    {
      prefix: queryKeys.videoCodec.library,
      keys: (id: string) => [
        queryKeys.videoCodec.asset(id, {
          id: 9,
          absPath: "/movie.mov",
          fileSize: 42,
          sourceMtime: 1,
          importStage: "done",
        }),
      ],
    },
    {
      prefix: queryKeys.assetMetadata.library,
      keys: (id: string) => [
        queryKeys.assetMetadata.asset(id, {
          id: 9,
          absPath: "/movie.mov",
          fileSize: 42,
          sourceMtime: 1,
          importStage: "done",
        }),
      ],
    },
    {
      prefix: queryKeys.importNotices.library,
      keys: (id: string) => [queryKeys.importNotices.library(id)],
    },
  ];
  for (const family of families) {
    const own = family.keys("a");
    const other = family.keys("b");
    for (const key of [...own, ...other]) client.setQueryData(key, key[1]);
    await client.invalidateQueries({ queryKey: family.prefix("a") });
    for (const key of own) expect(client.getQueryState(key)?.isInvalidated).toBe(true);
    for (const key of other) {
      expect(client.getQueryState(key)?.isInvalidated).toBe(false);
      expect(client.getQueryData(key)).toBe("b");
    }
  }
  client.clear();
});
