#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib>=3.10"]
# ///

import argparse
import csv
from pathlib import Path

import matplotlib.pyplot as plt


def read_rows(path: Path) -> list[dict[str, str]]:
    with path.open(newline="") as file:
        rows = list(csv.DictReader(file))
    if not rows:
        raise RuntimeError(f"{path} contains no basic-block rows")
    return rows


def linear_regression(x: list[int], y: list[int]) -> tuple[float, float, float]:
    mean_x = sum(x) / len(x)
    mean_y = sum(y) / len(y)
    x_variance = sum((value - mean_x) ** 2 for value in x)
    if x_variance == 0:
        raise RuntimeError("cannot regress against a constant independent variable")

    slope = sum(
        (x_value - mean_x) * (y_value - mean_y)
        for x_value, y_value in zip(x, y, strict=True)
    ) / x_variance
    intercept = mean_y - slope * mean_x
    residual_sum_squares = sum(
        (y_value - (slope * x_value + intercept)) ** 2
        for x_value, y_value in zip(x, y, strict=True)
    )
    total_sum_squares = sum((value - mean_y) ** 2 for value in y)
    r_squared = 1 - residual_sum_squares / total_sum_squares
    return slope, intercept, r_squared


def add_cost_plot(
    axis,
    x: list[int],
    y: list[int],
    x_label: str,
    y_label: str,
) -> tuple[float, float, float]:
    slope, intercept, r_squared = linear_regression(x, y)
    regression_x = sorted(set(x))
    regression_points = [
        (value, slope * value + intercept)
        for value in regression_x
        if slope * value + intercept >= 0
    ]

    axis.scatter(x, y, alpha=0.18, s=9, label="Basic blocks")
    if regression_points:
        line_x, line_y = zip(*regression_points, strict=True)
        axis.plot(
            line_x,
            line_y,
            color="tab:red",
            linewidth=2,
            label=f"OLS: y = {slope:.3g}x {intercept:+.3g}\n$R^2$ = {r_squared:.3f}",
        )
    axis.set_xscale("symlog", linthresh=1)
    axis.set_yscale("symlog", linthresh=1)
    axis.set_xlabel(x_label)
    axis.set_ylabel(y_label)
    axis.grid(alpha=0.2, which="both")
    axis.legend()
    return slope, intercept, r_squared


def plot_costs(rows: list[dict[str, str]], x_column: str, x_label: str, output: Path) -> None:
    x = [int(row[x_column]) for row in rows]
    costs = [
        ("assumed_gas", "Assumed scheduling gas"),
        ("assumed_code_bytes", "Assumed scheduling code size (bytes)"),
    ]

    figure, axes = plt.subplots(1, 2, figsize=(13, 5.5))
    for axis, (y_column, y_label) in zip(axes, costs, strict=True):
        y = [int(row[y_column]) for row in rows]
        slope, intercept, r_squared = add_cost_plot(axis, x, y, x_label, y_label)
        print(
            f"{x_column} -> {y_column}: "
            f"y = {slope:.8g}x {intercept:+.8g}; R^2 = {r_squared:.8g}"
        )

    figure.suptitle("Ordinary least-squares regressions in original units; axes use symmetric logs")
    figure.tight_layout()
    figure.savefig(output, dpi=180)
    plt.close(figure)


def main() -> None:
    parser = argparse.ArgumentParser(description="Plot SIR stack-scheduling corpus statistics")
    parser.add_argument("csv", type=Path)
    parser.add_argument("output_directory", type=Path)
    args = parser.parse_args()

    rows = read_rows(args.csv)
    args.output_directory.mkdir(parents=True, exist_ok=True)
    plot_costs(
        rows,
        "operation_count",
        "Basic-block operations",
        args.output_directory / "cost_vs_operations.png",
    )
    plot_costs(
        rows,
        "total_input_count",
        "Operation input operands + basic-block inputs",
        args.output_directory / "cost_vs_inputs.png",
    )


if __name__ == "__main__":
    main()
