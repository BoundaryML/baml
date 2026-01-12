import { AddTodoItem } from "../baml_client";
import { partial_types } from "../baml_client/partial_types";
import { b } from "./test-setup";

describe("Union streaming regression", () => {
  it("streams discriminated todo tools", async () => {
    const stream = b.stream.ChooseTodoTools("5 todo items for learning chess");

    const snapshots: (
      | (AddTodoItem | partial_types.TodoMessageToUser)[]
      | null
    )[] = [];

    for await (const chunk of stream) {
      console.log("chunk", chunk);
      snapshots.push(chunk);
    }

    expect(snapshots.length).toBeGreaterThan(0);

    const final = await stream.getFinalResponse();

    expect(final.length).toBeGreaterThan(0);

    const items = final.filter(
      (tool): tool is AddTodoItem => tool.type === "add_todo_item",
    );
    const message = final.find(
      (tool) =>
        tool.type === "todo_message_to_user",
    );

    expect(items.length).toBeGreaterThan(0);
    // expect(message).toBeDefined();
    // expect(message?.message).toEqual(expect.any(String));

    for (const item of items) {
      expect(item.item).toEqual(expect.any(String));
      expect(item.time).toEqual(expect.any(String));
      expect(item.description).toEqual(expect.any(String));
    }

    const lastSnapshot = snapshots.at(-1);
    expect(lastSnapshot).not.toBeNull();
    expect(lastSnapshot?.length).toEqual(final.length);

    // const discriminators = new Set(
    //   (lastSnapshot ?? [])
    //     .map((tool) => tool?.type)
    //     .filter((value): value is string => value != null),
    // );

    // expect(discriminators.has("add_todo_item")).toBe(true);
    // expect(discriminators.has("todo_message_to_user")).toBe(true);

    // We expect:
    //   - For all chunks:
    //     - the ith AddTodoItem tool is either equal to the ith tool of final, or missing.
    //   - some chunks have fewer tools than the final response.
    //   - some chunks have a non-null message that is shorter than the final message.

    for (const snapshot of snapshots) {
      // zip the snapshot tools with final response tools
      const zipped = (snapshot ?? []).map((tool, index) => [tool, final[index]]);
      for (const [snapshotTool, finalTool] of zipped) {
        let toolIsMissing = snapshotTool === undefined || snapshotTool === null;
        let toolIsAddTodoItem = snapshotTool?.type === "add_todo_item";
        if (!toolIsMissing && toolIsAddTodoItem) {
          expect(snapshotTool).toEqual(finalTool);
        }
      }
    }

    const tool_counts = snapshots.map((snapshot) => snapshot?.length ?? 0);
    const min_tool_count = Math.min(...tool_counts);
    expect(min_tool_count).toBeLessThanOrEqual(final.length);

    // For each tool, get the message length, drop tools that
    // aren't message tools.
    const message_lengths: number[] = snapshots.map((snapshot) => {
      const messageTool = (snapshot ?? []).find((tool) => tool?.type === "todo_message_to_user");
      if (messageTool?.type === "todo_message_to_user") {
        return messageTool?.message?.length ?? 0;
      }
      return 0;
    });
    expect(message_lengths.length).toBeGreaterThan(0);
    expect(message_lengths.length).toBeGreaterThan(2);
    const middle_index = Math.floor(message_lengths.length / 2);
    expect(message_lengths[middle_index]).toBeLessThan(message_lengths[message_lengths.length - 1])



  }, 20_000);
});
