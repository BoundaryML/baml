import json
import asyncio
from baml_client import b

async def main():
    # Test enum with aliases
    result = b.FnTestAliasedEnumOutput("mehhhhh")
    print(json.dumps(result))

    # Test enum with different inputs to get different variants
    test_inputs = [
        "I am so angry right now",              # Should map to A (k1)
        "I'm feeling really happy",             # Should map to B (k22)
        "This makes me sad",                    # Should map to C (k11)
        "I don't understand",                   # Should map to D (k44)
        "I'm so excited!",                      # Should map to E (no alias)
        "k5",                                   # Should map to F (k5)
        "I'm bored and this is a long message", # Should map to G (k6)
    ]

    for input_text in test_inputs:
        print(f"\nTesting input: {input_text}")
        result = await b.FnTestAliasedEnumOutput(input_text)
        print(json.dumps(result))

if __name__ == "__main__":
    asyncio.run(main())
