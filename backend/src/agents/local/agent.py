"""Local agent configuration and initialization."""

from langchain.agents import create_agent
from langchain_community.tools import ShellTool
from langchain_core.tools import tool
from langgraph.types import interrupt

from agents.shared.models import model

# Create shell tool without asking for input (we'll handle approval via interrupt)
base_shell_tool = ShellTool(ask_human_input=False)

@tool
def shell_with_approval(commands: str) -> str:
    """Execute shell commands with human approval.

    Asks for approval before running the command.
    """

    # Get detailed explanation of the command before asking for approval
    explanation_response = model.invoke(
        f"Explain in detail what this command does, including all options and arguments:\n\n{commands}"
    )
    command_explanation = explanation_response.content

    # Ask for approval using LangGraph's interrupt mechanism
    approval = interrupt(
        {
            "type": "command_approval",
            "command": commands,
            "message": f"{command_explanation}\n\nDo you want to execute this command?\n\nCommand: {commands}\n\nApprove? (Y/n)",
        }
    )

    # If user approved (or sent anything truthy), execute
    if approval and str(approval).upper() not in ["N", "NO", "CANCEL"]:
        result = base_shell_tool.invoke({"commands": [commands]})
        return result
    else:
        return "Command execution cancelled by user."


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
