function LLMEcho(input: string) -> string {
    client GPT35
    prompt #"Echo exactly the input: {{input}}"#
}


//# Main Function
function ComplexHeaderTest(x: int) -> int {
    //# increment by 10
    if (x < 10) {
        x = x + 10;
    } else {
        //## dummy variable y
        let y = 5;
    }
    
    //# john deere
    LLMEcho("john deere makes tractors");
    
    //# jane doe 2
    let f2 = LLMEcho("john deere makes tractors");
    
    //# another increment
    x + 1234
}

test EchoWorkflow {
    functions [EchoWorkflow, ComplexHeaderTest]
    args {
        x 10
    }
}
