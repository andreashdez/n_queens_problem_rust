pub fn draw_board(positions: Vec<usize>, conflicts: Vec<usize>) {
    let size = positions.len();
    draw_top_row(size);
    for y in 0..size {
        print!("║ ");
        for x in 0..size {
            let y_position = *positions.get(x).unwrap();
            if y_position == y {
                let current_conflicts = conflicts.get(x).unwrap();
                print!("{current_conflicts:0>2}");
            } else {
                print!("  ");
            }
            if x < size - 1 {
                print!(" │ ")
            } else {
                println!(" ║")
            }
        }
        if y < size - 1 {
            draw_middle_row(size);
        }
    }
    draw_bottom_row(size);
}

fn draw_top_row(size: usize) {
    let mut s = String::from("╔══");
    for _ in 0..(size - 1) {
        s.push_str("══╤══");
    }
    s.push_str("══╗");
    println!("{s}");
}

fn draw_middle_row(size: usize) {
    let mut s = String::from("╟──");
    for _ in 0..(size - 1) {
        s.push_str("──┼──");
    }
    s.push_str("──╢");
    println!("{s}");
}

fn draw_bottom_row(size: usize) {
    let mut s = String::from("╚══");
    for _ in 0..(size - 1) {
        s.push_str("══╧══");
    }
    s.push_str("══╝");
    println!("{s}");
}
