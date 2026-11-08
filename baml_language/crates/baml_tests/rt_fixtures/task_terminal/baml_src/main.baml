function RtCancelledFailure()->int {
    let failed=spawn {let n=4611686018427387903; n+1};
    let index=baml.future.__await_any([failed]);
    let cancelled=failed.cancel();
    if(cancelled || !failed.is_error() || index!=0) {return -1;}
    7
}

function RtLinkedCancelledFailure()->int {
    let token=baml.spawn.CancelToken.new();
    let failed=spawn with token {
        token.cancel();
        let n=4611686018427387903; n+1
    };
    7
}
