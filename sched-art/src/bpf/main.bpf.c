#include <scx/common.bpf.h>

char _license[] SEC("license") = "GPL";

UEI_DEFINE(uei);

static bool running;
struct {
    __uint(type, BPF_MAP_TYPE_ARRAY);
    __type(key, pid_t);
    __type(value, bool);
    __uint(max_entries, 1024);
} prior_tasks SEC(".maps");

static bool is_prior_task(pid_t pid)
{
    return bpf_map_lookup_elem(&prior_tasks, &pid);
}

s32 BPF_STRUCT_OPS(art_select_cpu, struct task_struct *p, s32 prev_cpu, u64 wake_flags) {
    if (is_prior_task(p->pid)) {
        bpf_printk("[sched-art] Prior task found: %d", p->pid);
    }

    return prev_cpu;
}

s32 BPF_STRUCT_OPS_SLEEPABLE(art_init)
{
    running = true;
    return 0;
}

void BPF_STRUCT_OPS(art_exit, struct scx_exit_info *ei)
{
    running = false;
    UEI_RECORD(uei, ei);
}

SCX_OPS_DEFINE(art_ops,
           .select_cpu      = (void *)art_select_cpu,
           .init            = (void *)art_init,
           .exit            = (void *)art_exit,
           .name            = "art");
