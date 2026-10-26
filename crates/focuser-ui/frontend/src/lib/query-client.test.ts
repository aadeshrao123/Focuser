import { onlineManager, type QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryClient } from "./query-client";

describe("local command scheduling", () => {
  let client: QueryClient;

  beforeEach(() => {
    onlineManager.setOnline(false);
    client = createQueryClient();
  });

  afterEach(() => {
    client.clear();
    onlineManager.setOnline(true);
  });

  it("loads local data when Windows reports offline", async () => {
    const read = vi.fn().mockResolvedValue([]);
    await expect(client.fetchQuery({ queryKey: ["block-lists"], queryFn: read })).resolves.toEqual(
      [],
    );
    expect(read).toHaveBeenCalledOnce();
    expect(client.getQueryState(["block-lists"])?.fetchStatus).toBe("idle");
  });

  it("runs local mutations offline instead of pausing them", async () => {
    const write = vi.fn().mockResolvedValue("saved");
    const mutation = client.getMutationCache().build(client, { mutationFn: write });
    await expect(mutation.execute(undefined)).resolves.toBe("saved");
    expect(write).toHaveBeenCalledOnce();
    expect(mutation.state.isPaused).toBe(false);
  });

  it("surfaces unavailable authority errors offline without retrying", async () => {
    const unavailable = new Error("Focuser authority unavailable");
    const read = vi.fn().mockRejectedValue(unavailable);
    await expect(client.fetchQuery({ queryKey: ["block-lists"], queryFn: read })).rejects.toBe(
      unavailable,
    );
    expect(read).toHaveBeenCalledOnce();
    expect(client.getQueryState(["block-lists"])?.status).toBe("error");
    expect(client.getQueryState(["block-lists"])?.fetchStatus).toBe("idle");
  });
});
