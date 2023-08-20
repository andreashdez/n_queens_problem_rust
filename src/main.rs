use crate::ga::chromosome::Chromosome;

pub mod ga;
pub mod ui;

fn main() {
    simple_logger::init().unwrap();
    let mut chromosome = Chromosome::new(8);
    chromosome.count_conflicts();
    let conflicts_sum = chromosome.get_conflicts_sum();
    println!("Conflicts: {conflicts_sum}");
    ui::draw_board(chromosome.get_positions(), chromosome.get_conflicts());
}
