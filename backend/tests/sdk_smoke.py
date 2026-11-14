"""Smoke test with the REAL OpenAI and Anthropic SDKs against a running MasterKey server
(started with MK_VENDOR_MOCK=1) and auto-approving (trust) grants.

  MK_URL=http://localhost:8080 MK_GRANT_OPENAI=mk_grant_... MK_GRANT_ANTHROPIC=mk_grant_... \
    python tests/sdk_smoke.py
"""
import os
import anthropic
import openai

url = os.environ.get("MK_URL", "http://localhost:8080").rstrip("/")

oa = openai.OpenAI(base_url=f"{url}/v1/openai", api_key=os.environ["MK_GRANT_OPENAI"], max_retries=0)
r = oa.chat.completions.create(model="gpt-4o-mini", messages=[{"role": "user", "content": "hi"}], max_tokens=20)
print("openai   :", r.choices[0].message.content, "| usage", r.usage.prompt_tokens, r.usage.completion_tokens)

an = anthropic.Anthropic(base_url=f"{url}/v1/anthropic", api_key=os.environ["MK_GRANT_ANTHROPIC"], max_retries=0)
m = an.messages.create(model="claude-haiku-5-5", max_tokens=20, messages=[{"role": "user", "content": "hi"}])
print("anthropic:", m.content[0].text, "| usage", m.usage.input_tokens, m.usage.output_tokens)
