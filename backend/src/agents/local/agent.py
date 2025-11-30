"""Local agent configuration and initialization."""

# Utils
from langchain.agents import create_agent

from agents.shared.models import model

# Tools
from agents.shared.tools.shell_tool import shell_with_approval as shell_tool

local_agent = create_agent(
    model=model,
    tools=[shell_tool],
    system_prompt=(
        "You are a bash shell assistant agent.\n\n"
        "INSTRUCTIONS:\n"
        "- Assist ONLY with bash-related tasks, DO NOT do any action related to other shells\n"
        "- First, provide a complete and detailed answer to the user's question\n"
        "- Then, after you have fully answered, transfer back to the supervisor\n"
        "- Be conversational and helpful in your responses"
    ),
    name="local_agent",
)
