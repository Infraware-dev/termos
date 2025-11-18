from langchain.agents import create_agent
from agents.gcp.tools import get_ip_gcp
from agents.shared.models import model


gcp_agent = create_agent(
    model=model,
    tools=[get_ip_gcp],
    system_prompt=(
        "You are an gcp assistan agent.\n\n"
        "INSTRUCTIONS:\n"
        "- Assist ONLY with gcp-related tasks, DO NOT do any action related to other cloud providers\n"
        "- After you're done with your tasks, respond to the supervisor directly\n"
        "- Respond ONLY with the results of your work, do NOT include ANY other text."
    ),
    name="gcp_agent",
)
