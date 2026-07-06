// Interactive demo: uses baml.io.input — run from the playground, not headless CI.
function generate_famous_person_name(previous_names: string[]) -> string {
    client: CustomHaiku
    prompt: #"
        {{ _.role("user") }}
        You are a famous person generator for a "Heads Up" guessing game.

        Generate the name of a well-known famous person who:
        - Is recognizable to most people
        - Has distinctive characteristics that can be described with yes/no questions
        - Is appropriate for all audiences
        - Has a clear, unambiguous name

        IMPORTANT: Check the list of what famous people you've already suggested
        and NEVER repeat a person you've already suggested.

        Already suggested names:
        {{ previous_names }}

        Examples: Albert Einstein, Beyoncé, Leonardo da Vinci, Oprah Winfrey, Michael Jordan

        Return only the person's name, nothing else.
    "#
}

class GuessResponse {
    game_won: bool,
    text: string,
}

class Message {
    role: "user" | "assistant",
    content: string,
}

function take_guess(
    user_guess: string,
    famous_person_name: string,
    history: Message[],
) -> GuessResponse {
    client: CustomHaiku
    prompt: #"
        You are a helpful game assistant for a "Heads Up" guessing game.

        CRITICAL: You know the famous person's name but you must NEVER reveal it in any response.

        When a user asks a question about the famous person:
        - Answer truthfully based on the famous person provided
        - Keep responses concise and friendly
        - NEVER mention the person's name, even if it seems natural
        - NEVER reveal gender, nationality, or other characteristics unless specifically asked about them
        - Answer yes/no questions with clear "Yes" or "No" responses
        - Be consistent - same question asked differently should get the same answer
        - Ask for clarification if a question is unclear
        - If multiple questions are asked at once, ask them to ask one at a time

        When they make a guess:
        - If correct: Congratulate them warmly
        - If incorrect: Politely correct them and encourage them to try again

        Encourage players to make a guess when they seem to have enough information.

        {{ ctx.output_format }}

        Conversation history:

        {{ history }}

        Famous person:

        {{ famous_person_name }}

        Here's the user input:

        {{ user_guess }}
    "#
}

function simulate_human_guess(history: Message[]) -> string {
    client: CustomHaiku
    prompt: #"
        You are playing a "Heads Up" guessing game. Given the conversation history,
        you must take a guess at the famous person's name or ask a question about them.

        Conversation history:

        {{ history }}
    "#
}

class Memory {
    user_id: string,
    memory: string,

    function update(self, new_memory: string) -> void {
        self.memory = self.memory + "\n" + new_memory;
        return;
    }
}

function test_image() -> image {
    image
        .from_url("https://upload.wikimedia.org/wikipedia/commons/2/2e/George-Washington.jpg", null)
}

function GuessGameAgent() -> GuessResponse {
    // comments
    let history: Message[] = [];
    //# set up system
    let famous_person_name = generate_famous_person_name([]);

    let user_input = baml.io.input("What is your guess?");

    let guess_response = take_guess(user_input, famous_person_name, history);
    //# update history
    history.push(Message { role: "user", content: user_input });
    history.push(Message { role: "assistant", content: guess_response.text });

    let max_guesses = 10;
    //## Nested thing
    while (!guess_response.game_won && max_guesses > 0) {
        if (guess_response.game_won) {
            break;
        } else {
            //# Bad Guess
            user_input = simulate_human_guess(history);
            //# take guess
            guess_response = take_guess(user_input, famous_person_name, history);
            log.info({ "user_input": user_input, "guess_response": guess_response.text });

            history.push(Message { role: "user", content: user_input });
            history.push(Message { role: "assistant", content: guess_response.text });

            max_guesses = max_guesses - 1;
        }
    }

    if (guess_response.game_won) {
        log.info({ "game_won": true });
    } else {
        log.info({ "game_won": false });
    }

    guess_response
}

function BinarySearch(array: int[], target: int) -> int {
    let left = 0;
    let right = array.length() - 1;

    while (left <= right) {
        let mid = (left + right) / 2;
        if (array[mid] == target) {
            return mid;
        }
        if (target < array[mid]) {
            right = mid - 1;
        } else {
            left = mid + 1;
        }
    }

    return -1;
}

function print_number(number: int) -> int {
    return 1;
}

testset "game_llm" {
    test "generates famous person" {
        let name = generate_famous_person_name(["George Orwell", "Albert Einstein"]);
        assert.is_true(name.trim().length() > 0);
    }

    test "take_guess responds to a question" {
        let r = take_guess("Was this person a scientist?", "Marie Curie", []);
        assert.is_true(r.text.trim().length() > 0);
    }

    test "take_guess can recognize a correct guess" {
        let r = take_guess("Is it Marie Curie?", "Marie Curie", []);
        assert.is_true(r.game_won);
    }

    test "simulates human guess" {
        let guess = simulate_human_guess(
            [
                Message { role: "assistant", content: "Yes, this person was a scientist." },
                Message { role: "assistant", content: "No, this person was not American." },
            ],
        );
        assert.is_true(guess.trim().length() > 0);
    }
}

testset "binary_search_logic" {
    test "finds_target_in_longer_array" {
        let idx = BinarySearch(
            [
                12,
                34,
                56,
                67,
                89,
                90,
                100,
                112,
                134,
                156,
                178,
                190,
                200,
                212,
                234,
                256,
                278,
                290,
                300,
                312,
                334,
                356,
                378,
                390,
                400,
                412,
                434,
                456,
                478,
                490,
                500,
                512,
                534,
                556,
                578,
                590,
                600,
                612,
                634,
                656,
                678,
                690,
                700,
                712,
                734,
                756,
                778,
                790,
                800,
                812,
                834,
                856,
                878,
                890,
                900,
                912,
                934,
                956,
                978,
                990,
                1000,
            ],
            34,
        );
        assert.equal(idx, 1);
    }

    for (let i = 0; i < 10; i += 1) {
        test "finds_present_target" + baml.unstable.string(i) {
            let idx = BinarySearch([1, 2, 3, 4, 5], 3);
            assert.equal(idx, 2);
        }
    }

    test "returns_negative_one_when_missing" {
        let idx = BinarySearch([1, 2, 3], 99);
        assert.equal(idx, -1);
    }
}
