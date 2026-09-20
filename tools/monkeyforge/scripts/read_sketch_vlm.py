"""Read a drawing with a vision-language model and return a garment spec as JSON.

This is the open-ended half of reading a sketch. CLIP retrieval (`monkeyforge.garments`) answers
closed questions well -- it scored a drawn suit's sleeves at 83% -- but it is measurably unable to
separate short sleeves from long ones on a crude doodle, and it cannot enumerate what was drawn or
read lettering at all. A VLM can do both, at the cost of sometimes inventing detail.

So this returns a *spec*, and the caller decides how much to trust each field. Sleeve length is
cross-checked against CLIP; where they disagree and CLIP is confident, CLIP wins, because its
confidence is calibrated and the VLM's prose is not.

Run (on a box with the weights):
    python scripts/read_sketch_vlm.py sketch.png
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

MODEL = "Qwen/Qwen2-VL-2B-Instruct"

#: One focused question per field, not one JSON blob.
#:
#: Asked for the whole schema at once, a 2B model slides answers between fields: on the t-shirt
#: drawing it answered "short" -- the right sleeve length -- but wrote it into `garment` and left
#: `sleeves` as "false". The information was there; the formatting destroyed it. Asking each
#: question on its own, with the permitted answers spelled out, removes the failure mode entirely
#: and costs only a few extra seconds of generation.
QUESTIONS: dict[str, str] = {
    "garment": (
        "What kind of clothing is this character wearing on its body? "
        "Answer with just the garment name, for example: t-shirt, suit jacket, hoodie, vest."
    ),
    "sleeves": (
        "Look at the character's arms. How far down the arms does the clothing go? "
        "Answer with EXACTLY ONE of these words and nothing else: sleeveless, short, long. "
        "Use 'sleeveless' if the arms are bare, 'short' if the sleeves stop near the shoulder, "
        "'long' if the sleeves reach the wrists."
    ),
    "text": (
        "Are there any letters or words written on the character's clothing? "
        "Answer with just the letters you can see, or the word NONE."
    ),
    "trousers": (
        "Is the character wearing trousers or long pants covering its legs? "
        "Answer with exactly one word: yes or no."
    ),
    "accessories": (
        "What is the character wearing that sticks out from its body, such as a hat, cap, "
        "sunglasses, or a tie? List them separated by commas, or answer NONE."
    ),
    # Protrusions are read as *places*, not as shapes. The builder grows spikes from the base's
    # own measured proportions, so all it needs from the drawing is which part of the body they
    # come out of -- a question a small model answers far more reliably than "describe the spikes".
    "spikes": (
        "Does this character have spikes, horns or sharp points sticking out of its body? "
        "If yes, answer with only the body parts they stick out of, from this list: "
        "shoulders, head, back, arms, forearms, tail. Separate with commas. "
        "If there are no spikes or horns, answer NONE."
    ),
}

#: Body-part words the builder knows how to grow spikes on.
SPIKE_REGIONS = ("shoulders", "head", "back", "arms", "forearms", "tail")

SLEEVES = ("sleeveless", "short", "long")

#: Ways a model says "nothing there". Treated as empty for both text and accessories.
NEGATIVES = frozenset({"NONE", "NO", "N/A", "NA", "NOTHING", "NIL", "", "-"})


def extract_json(text: str) -> dict:
    """Pull the first JSON object out of the reply.

    Small instruct models wrap JSON in prose or a code fence however firmly you ask them not to.
    """
    fenced = re.search(r"```(?:json)?\s*(\{.*?\})\s*```", text, re.S)
    candidate = fenced.group(1) if fenced else None
    if candidate is None:
        brace = re.search(r"\{.*\}", text, re.S)
        candidate = brace.group(0) if brace else None
    if candidate is None:
        raise ValueError(f"no JSON in reply: {text[:200]!r}")
    return json.loads(candidate)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("sketch", type=Path)
    parser.add_argument("--model", default=MODEL)
    parser.add_argument("--max-tokens", type=int, default=192)
    parser.add_argument("--raw", action="store_true", help="also print the model's reply verbatim")
    args = parser.parse_args()

    import torch
    from PIL import Image
    from transformers import AutoProcessor, Qwen2VLForConditionalGeneration

    processor = AutoProcessor.from_pretrained(args.model)
    model = Qwen2VLForConditionalGeneration.from_pretrained(
        args.model, torch_dtype=torch.float16, device_map="cuda"
    )

    image = Image.open(args.sketch).convert("RGB")

    def ask(question: str) -> str:
        messages = [{"role": "user",
                     "content": [{"type": "image"}, {"type": "text", "text": question}]}]
        prompt = processor.apply_chat_template(messages, tokenize=False,
                                               add_generation_prompt=True)
        inputs = processor(text=[prompt], images=[image], return_tensors="pt").to("cuda")
        # Greedy: this is extraction, and sampling only adds ways to be creatively wrong.
        generated = model.generate(**inputs, max_new_tokens=args.max_tokens, do_sample=False)
        return processor.batch_decode(
            [generated[0][inputs.input_ids.shape[1]:]], skip_special_tokens=True
        )[0].strip()

    answers = {field: ask(question) for field, question in QUESTIONS.items()}
    if args.raw:
        for field, answer in answers.items():
            print(f"RAW {field}: {answer!r}")

    spec: dict[str, object] = {"garment": answers["garment"].strip().rstrip(".").lower()}

    # Accept the word anywhere in the reply: the model often answers "short sleeves" rather than
    # the bare token, and refusing that would throw away a correct answer over punctuation.
    lowered = answers["sleeves"].lower()
    spec["sleeves"] = next((word for word in SLEEVES if word in lowered), "")
    if not spec["sleeves"]:
        spec["sleeves_raw"] = answers["sleeves"]

    # "NONE", "No", "No text", "N/A" all mean the same thing and none of them belong on a chest.
    # The model answered plain "No" on the suit drawing, which a NONE-only check would have
    # stamped across the jacket.
    text = answers["text"].strip().strip(".\"'")
    spec["text"] = "" if text.upper() in NEGATIVES or text.upper().startswith(
        ("NONE", "NO ", "N/A", "THERE IS NO", "THERE ARE NO")
    ) else text

    spec["trousers"] = answers["trousers"].strip().lower().startswith("yes")

    accessories = answers["accessories"].strip().rstrip(".")
    spec["accessories"] = [] if accessories.upper() in NEGATIVES or \
        accessories.upper().startswith(("NONE", "NO ", "NOTHING")) else [
            item.strip().lower() for item in accessories.split(",")
            if item.strip() and item.strip().upper() not in NEGATIVES
        ]

    # Only regions the builder actually knows are kept: an unrecognised word would otherwise
    # reach the assembler and fail there, long after the reason for it is visible.
    spikes = answers["spikes"].strip().rstrip(".").lower()
    spec["spikes"] = [] if spikes.upper() in NEGATIVES or spikes.upper().startswith(
        ("NONE", "NO ", "NOTHING")
    ) else [region for region in SPIKE_REGIONS if region in spikes]

    print(json.dumps(spec, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
