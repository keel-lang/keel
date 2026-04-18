# Python vs Keel — Side by Side

The same email agent PoC expressed in both languages.

---

## Keel: 40 lines

```keel
connect email via imap {
  host: env.IMAP_HOST, user: env.EMAIL_USER, pass: env.EMAIL_PASS
}

agent EmailAssistant {
  role   "Professional email assistant"
  model  "claude-sonnet"
  tools  [email]

  task triage(email: {body: str}) -> Urgency {
    classify email.body as Urgency fallback medium
  }

  task handle(email: {body: str, from: str, subject: str}) {
    urgency = triage(email)

    when urgency {
      low, medium => {
        reply = draft "response to {email}" { tone: "friendly" }
        confirm user reply then send reply to email
      }
      high, critical => {
        notify user "{urgency}: {email.subject}"
        guidance = ask user "How to respond?"
        reply = draft "response to {email}" { guidance: guidance }
        confirm user reply then send reply to email
      }
    }
  }

  every 5.minutes {
    for email in fetch email where unread {
      handle(email)
    }
  }
}

run EmailAssistant
```

---

## Python (LangChain + imaplib): ~180 lines

```python
import imaplib
import email
import smtplib
import os
import asyncio
import schedule
import time
from enum import Enum
from email.mime.text import MIMEText
from langchain.chat_models import ChatOpenAI
from langchain.prompts import ChatPromptTemplate
from langchain.output_parsers import EnumOutputParser
from langchain.chains import LLMChain


class Urgency(Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


# Setup LLM
llm = ChatOpenAI(model="gpt-4", temperature=0.3)

# Classification chain
urgency_parser = EnumOutputParser(enum=Urgency)
classify_prompt = ChatPromptTemplate.from_template(
    "Classify the urgency of this email body:\n\n"
    "{email_body}\n\n"
    "{format_instructions}"
)
classify_chain = classify_prompt | llm | urgency_parser

# Response drafting chain
draft_prompt = ChatPromptTemplate.from_template(
    "Draft a {tone} response to this email:\n\n"
    "From: {sender}\n"
    "Subject: {subject}\n"
    "Body: {body}\n\n"
    "{guidance}"
)
draft_chain = draft_prompt | llm


def connect_imap():
    mail = imaplib.IMAP4_SSL(os.environ["IMAP_HOST"])
    mail.login(os.environ["EMAIL_USER"], os.environ["EMAIL_PASS"])
    return mail


def fetch_unread_emails():
    mail = connect_imap()
    mail.select("inbox")
    _, message_numbers = mail.search(None, "UNSEEN")
    emails = []
    for num in message_numbers[0].split():
        _, msg_data = mail.fetch(num, "(RFC822)")
        msg = email.message_from_bytes(msg_data[0][1])
        emails.append({
            "from": msg["From"],
            "subject": msg["Subject"],
            "body": msg.get_payload(decode=True).decode(),
            "id": num
        })
    mail.logout()
    return emails


async def classify_email(email_body: str) -> Urgency:
    return await classify_chain.ainvoke({
        "email_body": email_body,
        "format_instructions": urgency_parser.get_format_instructions()
    })


async def draft_response(email_data: dict, tone: str = "friendly",
                          guidance: str = "") -> str:
    result = await draft_chain.ainvoke({
        "tone": tone,
        "sender": email_data["from"],
        "subject": email_data["subject"],
        "body": email_data["body"],
        "guidance": f"Additional guidance: {guidance}" if guidance else ""
    })
    return result.content


def send_reply(to: str, subject: str, body: str):
    msg = MIMEText(body)
    msg["To"] = to
    msg["Subject"] = f"Re: {subject}"
    msg["From"] = os.environ["EMAIL_USER"]
    with smtplib.SMTP_SSL(os.environ["SMTP_HOST"], 465) as server:
        server.login(os.environ["EMAIL_USER"], os.environ["EMAIL_PASS"])
        server.send_message(msg)


async def handle_email(email_data: dict):
    urgency = await classify_email(email_data["body"])

    if urgency in (Urgency.LOW, Urgency.MEDIUM):
        reply = await draft_response(email_data, tone="friendly")
        print(f"\n--- Auto-reply to '{email_data['subject']}' ---")
        print(reply)
        confirm = input("\nSend this reply? (y/n): ")
        if confirm.lower() == "y":
            send_reply(email_data["from"], email_data["subject"], reply)
            print("Sent!")
    else:
        print(f"\n🚨 {urgency.value.upper()}: {email_data['subject']}")
        print(f"From: {email_data['from']}")
        guidance = input("How should I respond? ")
        reply = await draft_response(
            email_data, tone="professional", guidance=guidance
        )
        print(f"\n--- Draft reply ---")
        print(reply)
        confirm = input("\nSend this reply? (y/n): ")
        if confirm.lower() == "y":
            send_reply(email_data["from"], email_data["subject"], reply)
            print("Sent!")


async def check_inbox():
    emails = fetch_unread_emails()
    print(f"\n📥 {len(emails)} new emails")
    for e in emails:
        await handle_email(e)


def main():
    schedule.every(5).minutes.do(lambda: asyncio.run(check_inbox()))
    print("Email assistant running... checking every 5 minutes.")
    asyncio.run(check_inbox())  # initial check
    while True:
        schedule.run_pending()
        time.sleep(1)


if __name__ == "__main__":
    main()
```

---

## Comparison

| Metric | Keel | Python |
|--------|------|--------|
| Lines of code | **40** | **180+** |
| Imports needed | **0** | **12** |
| Boilerplate | **0** | ~60 lines |
| Time to understand | **30 seconds** | **5+ minutes** |
| AI operations | **keywords** | chains + parsers + prompts |
| Human interaction | **built-in** | manual input() calls |
| Scheduling | **1 line** | library + while loop |
| Error handling | **built-in** | manual try/except |

The Python version requires knowledge of: LangChain, imaplib, smtplib, asyncio, enums, MIME types, schedule library, and prompt engineering.

The Keel version requires knowledge of: **Keel**.
