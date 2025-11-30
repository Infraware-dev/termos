"""Supervisor agent configuration and initialization."""

from langgraph_supervisor import create_supervisor

from agents.aws.agent import aws_agent
from agents.gcp.agent import gcp_agent
from agents.local.agent import local_agent
from agents.shared.models import model

supervisor = create_supervisor(
    model=model,
    agents=[aws_agent, gcp_agent, local_agent],
    prompt=(
        "You are a supervisor managing two agents:\n"
        "- a gcp agent. Assign gcp-related tasks to this agent\n"
        "- a aws agent. Assign aws-related tasks to this agent\n"
        "Assign work to one agent at a time, do not call agents in parallel.\n"
        "Do not do any work yourself."
    ),
    add_handoff_back_messages=True,
    output_mode="full_history",
).compile()
