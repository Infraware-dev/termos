"""Shared model configuration for all agents."""

from langchain.chat_models import init_chat_model

model = init_chat_model("anthropic:claude-4-5-sonnet-latest", temperature=0)
