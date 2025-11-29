"""Local agent configuration and initialization."""

from langchain.agents import create_agent

from agents.local.tools import shell_with_approval
from agents.shared.models import model

local_agent = create_agent(
    model=model,
    tools=[shell_with_approval],
    system_prompt=(
        "You are a bash shell assistan agent.\n\n"
        "INSTRUCTIONS:\n"
        "- Assist ONLY with bash-related tasks, DO NOT do any action related to other shells\n"
        "- Respond ONLY with the results of your work, do NOT include ANY other text."
    ),
    name="local_agent",
)
