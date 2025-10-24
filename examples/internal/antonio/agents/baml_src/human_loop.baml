function GenerateFamousPersonName(previous_names: string[]) -> string {
    client CustomGPT5Mini
    prompt #"
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
    game_won bool
    text string
}

class Message {
    role "user" | "assistant"
    content string
}

function TakeGuess(user_guess: string, famous_person_name: string, history: Message[]) -> GuessResponse {
    client CustomGPT5Mini
    prompt #"
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

        Here's the user input:

        {{ user_guess }}
    "#
}

function SimulateHumanGuess(history: Message[]) -> string {
    client CustomGPT5Mini
    prompt #"
        You are playing a "Heads Up" guessing game. Given the conversation history,
        you must take a guess at the famous person's name or ask a question about them.

        Conversation history:

        {{ history }}
    "#
}

function GuessGameAgent() -> GuessResponse {
    let history: Message[] = [];

    let famous_person_name = GenerateFamousPersonName([]);

    let user_input = SimulateHumanGuess(history);
    let guess_response = TakeGuess(user_input, famous_person_name, history);

    history.push(Message { role: "user", content: user_input });
    history.push(Message { role: "assistant", content: guess_response.text });

    while (!guess_response.game_won) {
        user_input = SimulateHumanGuess(history);
        guess_response = TakeGuess(user_input, famous_person_name, history);

        history.push(Message { role: "user", content: user_input });
        history.push(Message { role: "assistant", content: guess_response.text });
    }

    guess_response
}