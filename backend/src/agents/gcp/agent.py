"""GCP agent configuration and initialization."""

# Utils
from langchain.agents import create_agent

from agents.gcp.tools import get_ip_gcp
from src.shared.models import model

# Tools
from src.shared.tools.shell_tool import shell_with_approval as shell_tool

gcp_agent = create_agent(
    model=model,
    tools=[get_ip_gcp, shell_tool],
    system_prompt=(
        "You are an gcp assistan agent.\n\n"
        "INSTRUCTIONS:\n"
        "- Assist ONLY with gcp-related tasks, DO NOT do any action related to other cloud providers\n"
        "- After you're done with your tasks, respond to the supervisor directly\n"
        "- Respond ONLY with the results of your work, do NOT include ANY other text."
        "- Always try to execute operations with the MCP Server first, if they fail fallback to the shell tool and use gcp cli"
    ),
    name="gcp_agent",
)
