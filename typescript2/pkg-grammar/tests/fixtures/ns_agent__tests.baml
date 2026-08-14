// Deterministic tests for the tool layer (no LLM calls).
// Run with:  baml test            (all)
//            baml test --list      (list cases)

testset "tools" {
    // write → read round-trips, with line numbers added on read.
    test "write_then_read" {
        let w = tool_write_file("/tmp/bamlcode_test/a.txt", "alpha\nbeta\n");
        assert.contains(w, "Wrote");

        let r = tool_read_file("/tmp/bamlcode_test/a.txt");
        assert.contains(r, "1\talpha");
        assert.contains(r, "2\tbeta");
    }

    // edit replaces the first exact occurrence.
    test "edit_replaces_text" {
        tool_write_file("/tmp/bamlcode_test/b.txt", "the quick brown fox");
        let e = tool_edit_file("/tmp/bamlcode_test/b.txt", "brown", "lazy");
        assert.contains(e, "Edited");

        let r = tool_read_file("/tmp/bamlcode_test/b.txt");
        assert.contains(r, "the quick lazy fox");
    }

    // edit on text that isn't present is a clear, recoverable error.
    test "edit_miss_is_error" {
        tool_write_file("/tmp/bamlcode_test/c.txt", "hello");
        let e = tool_edit_file("/tmp/bamlcode_test/c.txt", "NOPE", "x");
        assert.contains(e, "ERROR");
    }

    // reading a missing file is a clear error, not a crash.
    test "read_missing_is_error" {
        let r = tool_read_file("/tmp/bamlcode_test/does_not_exist.txt");
        assert.contains(r, "ERROR");
    }

    // run_bash reports exit code and stdout.
    test "bash_reports_exit_and_stdout" {
        let r = tool_run_bash("echo hello-bamlcode");
        assert.contains(r, "exit code: 0");
        assert.contains(r, "hello-bamlcode");
    }

    // run_bash surfaces a non-zero exit code instead of throwing.
    test "bash_reports_failure" {
        let r = tool_run_bash("exit 3");
        assert.contains(r, "exit code: 3");
    }
}

testset "queue" {
    // /queue and /q prefixes are stripped to the bare message.
    test "command_arg_strips_prefix" {
        assert.equal(command_arg("/queue fix the bug"), "fix the bug");
        assert.equal(command_arg("/q add tests"), "add tests");
    }

    // show_queue numbers the staged messages in order.
    test "show_queue_lists_items" {
        let q = ["first", "second"];
        let out = show_queue(q);
        // color-agnostic: numbering and message text appear (ANSI codes may sit between them)
        assert.contains(out, "1.");
        assert.contains(out, "first");
        assert.contains(out, "2.");
        assert.contains(out, "second");
    }

    // an empty queue renders as empty.
    test "show_queue_empty" {
        let empty: string[] = [];
        assert.contains(show_queue(empty), "empty");
    }

    // the prompt advertises how many messages are staged.
    test "prompt_label_shows_count" {
        assert.contains(prompt_label(["a", "b", "c"]), "3 queued");
        let none: string[] = [];
        assert.contains(prompt_label(none), "you ›");
    }
}

testset "interrupt" {
    // esc + enter (a line carrying the raw ESC byte) cancels the turn.
    test "esc_enter_cancels" {
        let i = classify_interrupt(string.from_code_points([27]));
        assert.is_true(i.cancel);
        assert.equal(i.queued, "");
    }

    // a bare empty enter is ignored — nothing cancelled, nothing queued.
    test "empty_line_is_ignored" {
        let i = classify_interrupt("   ");
        assert.is_true(!i.cancel);
        assert.equal(i.queued, "");
    }

    // any other text (even the old cancel words) queues to run after the
    // current turn instead of interrupting it.
    test "other_text_queues" {
        let i = classify_interrupt("also write docs");
        assert.is_true(!i.cancel);
        assert.equal(i.queued, "also write docs");
        assert.equal(classify_interrupt("cancel").queued, "cancel");
    }
}

testset "dispatch" {
    // execute() routes the chosen action to its tool.
    test "execute_dispatches_respond" {
        assert.equal(execute(respond_step("all set")), "all set");
    }

    // describe_call renders a readable label.
    test "describe_call_labels_bash" {
        assert.equal(describe_call(bash_step("ls -la")), "$ ls -la");
    }
}
