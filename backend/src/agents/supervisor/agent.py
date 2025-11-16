from langgraph_supervisor import create_supervisor
from langchain.chat_models import init_chat_model
from agents.aws.agent import aws_agent
from agents.gcp.agent import gcp_agent

model = init_chat_model(
    "anthropic:claude-3-7-sonnet-latest",
    temperature=0
)

supervisor = create_supervisor(
    model=model,
    agents=[aws_agent, gcp_agent],
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

