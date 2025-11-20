from langchain.chat_models import init_chat_model

model = init_chat_model("anthropic:claude-3-7-sonnet-latest", temperature=0)