from baml_client import b


def stock_agent():
    prompt = input("Ask LLM about stock price of a company: ")
    response = b.StockAgent(prompt)
    print(response)


def recruiter_agent():
    resume_text = input("Resume text: ")
    response = b.Recruiter(resume_text)
    print(response)


def human_loop_agent():
    response = b.GuessGameAgent()
    print(response)

def planning_agent():
    user_input = input("Where do you want to plan activities?: ")
    response = b.ActivityPlanningWorkflow(user_input)
    print(response)


def main():
    agent = input("Which agent do you want to run? (stock_agent, recruiter_agent, human_loop_agent, planning_agent): ")
    if agent == "stock_agent":
        stock_agent()
    elif agent == "recruiter_agent":
        recruiter_agent()
    elif agent == "human_loop_agent":
        human_loop_agent()
    elif agent == "planning_agent":
        planning_agent()
    else:
        print("Invalid agent")


if __name__ == "__main__":
    main()
