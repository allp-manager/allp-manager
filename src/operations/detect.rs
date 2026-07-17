use crate::{cli::Renderer, discovery::DiscoveryReport};

pub fn run(renderer: &Renderer, report: &DiscoveryReport, verbose: bool) {
    renderer.detection(report, verbose);
}
