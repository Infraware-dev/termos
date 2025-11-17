from langchain.agents import create_agent
from langchain.chat_models import init_chat_model
from agents.aws.tools import get_ip_aws

model = init_chat_model(
    "anthropic:claude-3-7-sonnet-latest",
    temperature=0
)

aws_agent = create_agent(
    model=model,
    tools=[get_ip_aws],
    system_prompt=(
        "You are an AWS assistan agent.\n\n"
        "INSTRUCTIONS:\n"
        "- Assist ONLY with AWS-related tasks, DO NOT do any action related to other cloud providers\n"
        "- After you're done with your tasks, respond to the supervisor directly\n"
        "- Respond ONLY with the results of your work, do NOT include ANY other text."
    ),
    name="aws_agent",
)