from baml_client import b


def stock_agent():
    prompt = input("Prompt: ")
    response = b.StockAgent(prompt)
    print(response)


def recruiter_agent():
    resume_text = input("Resume text: ")
    response = b.Recruiter(resume_text)
    print(response)


def human_loop_agent():
    response = b.GuessGameAgent()
    print(response)


def main():
    human_loop_agent()


if __name__ == "__main__":
    main()
