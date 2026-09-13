---
title: "How the algorithm works"
description: "Understand board representation, genetic operators, and the metrics used to follow a run."
---

The solver searches a population of possible boards for one with no attacking queens. It uses a genetic algorithm, with optional local search to improve promising candidates.

## A board is a permutation

Each entry gives the row of the queen in that column. Every row appears exactly once, so queens cannot share a row or column. Only diagonal conflicts remain. Two queens conflict when their column distance equals their row distance.

The solver's arrays use zero-based positions. GUI coordinates use one-based row and column labels.

## One epoch at a time

1. **Select parents.** Roulette selection uses fitness weights; tournament selection chooses the best candidate from a random group. Parents come from the population at the start of mating.
2. **Create offspring.** Partially mapped crossover (PMX) combines two parent permutations while retaining one queen per row and column.
3. **Mutate.** Swap row positions in selected non-elite chromosomes to explore other boards.
4. **Improve locally, if enabled.** Try random swaps and keep improvements in a configured fraction of non-elite candidates.
5. **Select survivors.** Keep elites and sample other survivors to return to the target population size.
6. **Measure and refresh.** Recalculate fitness and diversity. If too many boards are duplicates, refresh non-elites with random permutations, then record the epoch's metrics.

The initial population is recorded as **epoch zero**. The best board found so far is retained across epochs.

## When progress stalls

Stagnation counts epochs without improving the best conflict score. As it increases, the solver adjusts mutation and elite rates to encourage exploration. At the restart threshold, a **soft restart** preserves a small elite group and replenishes the population before continuing evolution.

A diversity refresh and a soft restart are different events. **R** markers in GUI charts identify actual soft-restart epochs.

## Reading the metrics

| Metric | Meaning |
| --- | --- |
| Best conflicts | Best conflict sum found so far. Zero means solved. Each attacking pair contributes to both queens' counts. |
| Average conflicts | Mean conflict sum of the current population. |
| Diversity ratio | Unique board permutations divided by population size. |
| Mutation / elite ratio | The rates applied for that epoch, including adaptive changes. |
| Stagnation epochs | Epochs since the latest improvement or stagnation reset. |
| Local-search improvements | Candidates improved by local search during the epoch. |
| Restart count | Cumulative soft restarts completed so far. |

The GUI retains a bounded sample of history. A line between retained points does not mean every intermediate epoch was kept. [The GUI guide](../gui/#read-the-charts) explains chart sampling and exports.

## Why use multiple seeds?

A seed makes a run reproducible for the same implementation and dependencies. It does not show whether a setting works well across different starting populations. Compare multiple seeds by solve rate, then runtime and solved epoch.

[Choose parameters →](../tuning/) · [Inspect measured results →](../benchmarks/)
