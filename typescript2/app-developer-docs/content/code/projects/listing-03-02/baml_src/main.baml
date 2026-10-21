// docs:start all
function main() -> int {
    // docs:start shadow
    let transcript = "  What is your refund policy?  ";
    let transcript = transcript.trim();
    let transcript = transcript.length();
    // docs:end shadow
    transcript
}
// docs:end all

test "chapter example" {
    assert.equal(main(), 27)
}
