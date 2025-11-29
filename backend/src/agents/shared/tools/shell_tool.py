"""Tools for shell command execution with human approval."""

from langchain_community.tools import ShellTool
from langchain_core.tools import tool
from langgraph.types import interrupt

# Create shell tool without asking for input (we'll handle approval via interrupt)
base_shell_tool = ShellTool()


@tool
def shell_with_approval(commands: str) -> str:
    """Execute shell commands with human approval.

    Asks for approval before running the command.
    """
    ## Get detailed explanation of the command before asking for approval (TBI)
    # explanation_response = model.invoke(
    #    f"Explain in detail what this command does, including all options and arguments:\n\n{commands}"
    # )
    # command_explanation = explanation_response.content

    # Ask for approval using LangGraph's interrupt mechanism
    approval = interrupt(
        {
            "type": "command_approval",
            "command": commands,
            # "message": f"{command_explanation}\n\nDo you want to execute this command?\n\nCommand: {commands}\n\nApprove? (Y/n)",
            "message": f"Do you want to execute this command?\n\nCommand: {commands}\n\nApprove? (Y/n)",
        }
    )

    # If user approved (or sent anything truthy), execute
    if approval and str(approval).upper() not in ["N", "NO", "CANCEL"]:
        result = base_shell_tool.invoke({"commands": [commands]})
        return result
    else:
        return "Command execution cancelled by user."
