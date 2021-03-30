// Copyright © 2021 Intel Corporation
//
// SPDX-License-Identifier: Apache-2.0
//

#[cfg(test)]
#[macro_use]
extern crate lazy_static;

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;
    use std::thread;
    use std::{ffi::OsStr, path::PathBuf};
    use test_infra::{
        DiskConfig, DiskType, GuestNetworkConfig, UbuntuDiskConfig, WaitForBootError,
    };

    use uuid::Uuid;
    use vmm_sys_util::tempdir::TempDir;

    const FOCAL_IMAGE_NAME: &str = "focal-server-cloudimg-amd64-custom-20210106-1.raw";

    pub const DEFAULT_TCP_LISTENER_PORT: u16 = 8000;

    lazy_static! {
        static ref NEXT_VM_ID: Mutex<u8> = Mutex::new(1);
    }

    struct Guest<'a> {
        tmp_dir: TempDir,
        vm_name: String,
        uuid: String,
        kernel_path: PathBuf,
        network: GuestNetworkConfig,
        disk_config: &'a dyn DiskConfig,
    }

    impl<'a> std::panic::RefUnwindSafe for Guest<'a> {}

    impl<'a> Guest<'a> {
        fn create_domain(&self, memory_size: u64) -> PathBuf {
            let domain = format!(
                "<domain type='ch'> \
        <name>{}</name> \
        <uuid>{}</uuid> \
        <genid>43dc0cf8-809b-4adb-9bea-a9abb5f3d90e</genid> \
        <title>Test VM {}</title> \
        <description>Test VM {}</description> \
        <os> \
                <type>hvm</type> \
                <kernel>{}</kernel> \
        </os> \
        <memory unit='b'>{}</memory> \
        <devices> \
                <disk type='file'> \
                        <source file='{}'/> \
                        <target dev='vda' bus='virtio'/> \
                </disk> \
                <disk type='file'> \
                        <source file='{}'/> \
                        <target dev='vdb' bus='virtio'/> \
                </disk> \
                <console type='pty'> \
                        <target type='virtio' port='0'/> \
                </console> \
                <interface type='ethernet'> \
                        <mac address='{}'/> \
                        <model type='virtio'/> \
                        <source> \
                               <ip address='{}' prefix='24'/> \
                        </source> \
              </interface> \
        </devices> \
        </domain>",
                self.vm_name,
                self.uuid,
                self.vm_name,
                self.vm_name,
                self.kernel_path.to_str().unwrap(),
                memory_size,
                self.disk_config
                    .disk(DiskType::OperatingSystem)
                    .unwrap()
                    .as_str(),
                self.disk_config.disk(DiskType::CloudInit).unwrap().as_str(),
                self.network.guest_mac,
                self.network.host_ip,
            );

            eprintln!("{}\n", domain);

            let mut domain_path = self.tmp_dir.as_path().to_path_buf();
            domain_path.push("domain.xml");

            let mut f = std::fs::File::create(&domain_path).unwrap();
            f.write_all(&domain.as_bytes()).unwrap();

            domain_path
        }

        fn new_from_ip_range(disk_config: &'a mut dyn DiskConfig, class: &str, id: u8) -> Self {
            let tmp_dir = TempDir::new_with_prefix("/tmp/ch").unwrap();

            let mut workload_path = dirs::home_dir().unwrap();
            workload_path.push("workloads");

            let mut kernel_path = workload_path;
            #[cfg(target_arch = "aarch64")]
            kernel_path.push("Image");
            #[cfg(target_arch = "x86_64")]
            kernel_path.push("hypervisor-fw");
            let network = GuestNetworkConfig {
                guest_ip: format!("{}.{}.2", class, id),
                l2_guest_ip1: format!("{}.{}.3", class, id),
                l2_guest_ip2: format!("{}.{}.4", class, id),
                l2_guest_ip3: format!("{}.{}.5", class, id),
                host_ip: format!("{}.{}.1", class, id),
                guest_mac: format!("12:34:56:78:90:{:02x}", id),
                l2_guest_mac1: format!("de:ad:be:ef:12:{:02x}", id),
                l2_guest_mac2: format!("de:ad:be:ef:34:{:02x}", id),
                l2_guest_mac3: format!("de:ad:be:ef:56:{:02x}", id),
                tcp_listener_port: DEFAULT_TCP_LISTENER_PORT + id as u16,
            };

            disk_config.prepare_files(&tmp_dir, &network);

            let vm_name = format!("vm-{}", id);
            Guest {
                tmp_dir,
                disk_config,
                kernel_path,
                network,
                vm_name,
                uuid: Uuid::new_v4().to_hyphenated().to_string(),
            }
        }

        fn new(disk_config: &'a mut dyn DiskConfig) -> Self {
            let mut guard = NEXT_VM_ID.lock().unwrap();
            let id = *guard;
            *guard = id + 1;

            Self::new_from_ip_range(disk_config, "192.168", id)
        }

        #[allow(dead_code)]
        fn wait_vm_boot(&self, custom_timeout: Option<i32>) -> Result<(), WaitForBootError> {
            self.network.wait_vm_boot(custom_timeout)
        }
    }

    fn spawn_libvirtd() -> io::Result<Child> {
        Command::new("libvirtd")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }

    fn spawn_virsh<I, S>(args: I) -> io::Result<Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Command::new("virsh")
            .args(&["-c", "ch:///system"])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }

    fn cleanup_libvirt_state() {
        let _ = std::fs::remove_dir_all("/var/lib/libvirt");
        let _ = std::fs::remove_file("/var/run/libvirtd.pid");
        let _ = std::fs::remove_dir_all("/var/run/libvirt");
    }

    #[test]
    fn test_create_vm() {
        cleanup_libvirt_state();
        let mut libvirtd = spawn_libvirtd().unwrap();
        thread::sleep(std::time::Duration::new(5, 0));

        let mut disk = UbuntuDiskConfig::new(FOCAL_IMAGE_NAME.to_owned());
        let guest = Guest::new(&mut disk);

        let domain_path = guest.create_domain(1 << 30);

        let r = std::panic::catch_unwind(|| {
            let output = spawn_virsh(&["create", domain_path.to_str().unwrap()])
                .unwrap()
                .wait_with_output()
                .unwrap();

            eprintln!(
                "create stdout\n\n{}\n\ncreate stderr\n\n{}",
                std::str::from_utf8(&output.stdout).unwrap(),
                std::str::from_utf8(&output.stderr).unwrap()
            );

            assert!(std::str::from_utf8(&output.stdout)
                .unwrap()
                .trim()
                .starts_with(&format!("Domain {} created", guest.vm_name)));
        });

        thread::sleep(std::time::Duration::new(15, 0));

        spawn_virsh(&["destroy", &guest.vm_name])
            .unwrap()
            .wait()
            .unwrap();

        libvirtd.kill().unwrap();
        let libvirtd_output = libvirtd.wait_with_output().unwrap();

        eprintln!(
            "libvirtd stdout\n\n{}\n\nlibvirtd stderr\n\n{}",
            std::str::from_utf8(&libvirtd_output.stdout).unwrap(),
            std::str::from_utf8(&libvirtd_output.stderr).unwrap()
        );

        assert!(r.is_ok());
    }

    #[test]
    fn test_uri() {
        cleanup_libvirt_state();
        let mut libvirtd = spawn_libvirtd().unwrap();
        thread::sleep(std::time::Duration::new(5, 0));

        let output = spawn_virsh(&["uri"]).unwrap().wait_with_output().unwrap();

        libvirtd.kill().unwrap();
        let libvirtd_output = libvirtd.wait_with_output().unwrap();

        eprintln!(
            "libvirtd stdout\n\n{}\n\nlibvirtd stderr\n\n{}",
            std::str::from_utf8(&libvirtd_output.stdout).unwrap(),
            std::str::from_utf8(&libvirtd_output.stderr).unwrap()
        );

        assert_eq!(
            std::str::from_utf8(&output.stdout).unwrap().trim(),
            "ch:///system"
        );
    }
}
