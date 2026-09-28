//! 自适应采样逻辑的单元测试（仅通过公开API访问，不依赖设备文件系统）

use crate::model::gpu::GPU;

fn interval(gpu: &GPU) -> u64 {
    gpu.frequency_strategy.get_sampling_interval()
}

#[test]
fn large_load_jump_uses_min_interval() {
    let mut gpu = GPU::new();
    gpu.set_adaptive_sampling(true, 2, 20, 8);
    gpu.adjust_sampling_interval_by_load(10);
    gpu.adjust_sampling_interval_by_load(90);
    assert_eq!(interval(&gpu), 2);
}

#[test]
fn steady_load_uses_max_interval() {
    let mut gpu = GPU::new();
    gpu.set_adaptive_sampling(true, 2, 20, 8);
    gpu.adjust_sampling_interval_by_load(50);
    gpu.adjust_sampling_interval_by_load(52);
    assert_eq!(interval(&gpu), 20);
}

#[test]
fn moderate_change_interpolates() {
    let mut gpu = GPU::new();
    gpu.set_adaptive_sampling(true, 2, 20, 8);
    gpu.adjust_sampling_interval_by_load(50);
    gpu.adjust_sampling_interval_by_load(65); // diff 15 -> ratio 0.5 -> 20 - 9
    assert_eq!(interval(&gpu), 11);
}

#[test]
fn disabled_keeps_fixed_interval() {
    let mut gpu = GPU::new();
    gpu.set_adaptive_sampling(false, 0, 0, 8);
    gpu.adjust_sampling_interval_by_load(10);
    gpu.adjust_sampling_interval_by_load(90);
    assert_eq!(interval(&gpu), 8);
}

/// 运行时开启自适应采样后，第一次比较不应使用过期的last_load
#[test]
fn enabling_at_runtime_uses_fresh_last_load() {
    let mut gpu = GPU::new();
    gpu.set_adaptive_sampling(false, 0, 0, 8);
    for _ in 0..5 {
        gpu.adjust_sampling_interval_by_load(80); // 负载稳定在80%
    }
    gpu.set_adaptive_sampling(true, 2, 20, 8);
    gpu.adjust_sampling_interval_by_load(80); // 依旧是80%，变化为0
    assert_eq!(interval(&gpu), 20);
}

/// 配置中 min > max 不应导致 clamp panic
#[test]
fn inverted_min_max_does_not_panic() {
    let mut gpu = GPU::new();
    gpu.set_adaptive_sampling(true, 30, 10, 8);
    gpu.adjust_sampling_interval_by_load(10);
    gpu.adjust_sampling_interval_by_load(90);
    gpu.adjust_sampling_interval_by_load(95);
    let i = interval(&gpu);
    assert!((10..=30).contains(&i));
}
