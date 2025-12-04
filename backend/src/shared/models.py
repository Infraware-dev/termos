"""Shared model configuration for all agents."""

from langchain.chat_models import init_chat_model

model = init_chat_model("anthropic:claude-sonnet-4-20250514", temperature=0)
