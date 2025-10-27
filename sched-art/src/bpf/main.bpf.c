#include <scx/common.bpf.h>
#include <vmlinux.h>

char _license[] SEC("license") = "GPL";

UEI_DEFINE(uei);

static bool running;
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __type(key, pid_t);
    __type(value, u8);
    __uint(max_entries, 1024);
} prior_tasks SEC(".maps");

static bool is_prior_task(pid_t pid)
{
    if (bpf_map_lookup_elem(&prior_tasks, &pid) == NULL) {
        return false;
    } else {
        return true;
    }
}

s32 BPF_STRUCT_OPS(art_select_cpu, struct task_struct *p, s32 prev_cpu, u64 wake_flags) {
    if (is_prior_task(p->pid)) {
        bpf_printk("[sched-art] Prior task found: %d\n", p->pid);
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
