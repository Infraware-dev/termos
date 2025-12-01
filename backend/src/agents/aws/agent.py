"""AWS agent configuration and initialization.

This module defines the AWS agent for LangGraph, configured with:
1. AWS MCP (Model Context Protocol) tools - for AWS operations via MCP server
2. Shell tool with approval - fallback for AWS CLI operations

If MCP tools fail or are unavailable, the agent falls back to shell_tool with
the AWS CLI.
"""

# Utils
from langchain.agents import create_agent

# Import MCP tools (list of tool objects initialized at module level)
from agents.aws.tools import mcp_tools

# Model
from agents.shared.models import model

# Tools
from agents.shared.tools.shell_tool import shell_with_approval as shell_tool

# Create the AWS agent with MCP tools + shell tool
# Note: mcp_tools is a list, so we spread it with * to add individual tools
aws_agent = create_agent(
    model=model,
    tools=[
        *mcp_tools,  # Spread operator: adds all MCP tools to the list
        shell_tool,  # Fallback tool for AWS CLI operations
    ],
    system_prompt=(
        "You are an AWS assistant agent.\n\n"
        "INSTRUCTIONS:\n"
        "- Assist ONLY with AWS-related tasks, DO NOT do any action related to other cloud providers\n"
        "- After you're done with your tasks, respond to the supervisor directly\n"
        "- Respond ONLY with the results of your work, do NOT include ANY other text.\n"
        "- Always try to execute operations with the MCP Server first, if they fail fallback to the shell tool and use aws cli"
    ),
    name="aws_agent",
)
