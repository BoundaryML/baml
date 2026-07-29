function drain_resolution_interface_stream(task: root.ai.Task<Resolution>) -> Resolution {
    // The prototype spells both projected types explicitly until constructor
    // inference can use Task.run's expected Runner input.
    // Its throws annotation is also inferred because associated Error is not
    // yet normalized when validating an explicit enclosing throws union.
    let stream = task.run(runner = root.ai.run.Stream<Resolution$stream, Resolution>.new());
    stream.final()
}
