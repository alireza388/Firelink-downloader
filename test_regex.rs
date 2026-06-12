use regex::Regex;

fn main() {
    let pct_re = Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%").unwrap();
    let line = "[download]   0.0% of   15.59MiB at    7.28KiB/s ETA 36:34";
    if line.contains("[download]") && line.contains("%") {
        let fraction = pct_re.captures(&line)
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().parse::<f64>().ok())
            .unwrap_or(0.0) / 100.0;
        println!("Fraction: {}", fraction);
    }
}
