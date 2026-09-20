from pathlib import Path

from monkeyforge.ranking import load_preferences, train_pairwise_ranker


def test_example_preferences_train_with_perfect_pairwise_accuracy() -> None:
    dataset = Path(__file__).parents[1] / "training" / "dataset.example.jsonl"
    examples = load_preferences(dataset)
    artifact = train_pairwise_ranker(examples, epochs=200)
    assert artifact.example_count == 4
    assert artifact.pairwise_accuracy == 1.0
    assert artifact.final_loss < 0.5
