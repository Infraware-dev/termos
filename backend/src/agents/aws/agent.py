from langchain.agents import create_agent
from agents.aws.tools import get_ip_aws
from agents.shared.models import model
from langchain_community.tools import ShellTool

shell_tool = ShellTool(
    ask_human_input=True
)

aws_agent = create_agent(
    model=model,
    tools=[get_ip_aws, shell_tool],
    system_prompt=(
        "You are an AWS assistan agent.\n\n"
        "INSTRUCTIONS:\n"
        "- Assist ONLY with AWS-related tasks, DO NOT do any action related to other cloud providers\n"
        "- After you're done with your tasks, respond to the supervisor directly\n"
        "- Respond ONLY with the results of your work, do NOT include ANY other text."
        "- Always try to execute operations with the MCP Server first, if they fail fallback to the shell tool and use aws cli"
    ),
    name="aws_agent",
)
