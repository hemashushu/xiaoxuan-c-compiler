# References

## Pointer Authentication

- [Understand Arm Pointer Authentication](https://learn.arm.com/learning-paths/servers-and-cloud-computing/pac/)

## Memory Tagging Extension

- [Armv8.5-A Memory Tagging Extension White Paper](https://support.arm.com/documentation/102925/latest/)
- [Introduction to the Memory Tagging Extension](https://support.arm.com/documentation/108035/0100/Introduction-to-the-Memory-Tagging-Extension)
- [ChkTag: x86 Memory Safety](https://community.intel.com/t5/Blogs/Tech-Innovation/open-intel/ChkTag-x86-Memory-Safety/post/1721490)
- [Enable Memory Tagging Extension on Google Pixel 8](https://learn.arm.com/learning-paths/mobile-graphics-and-gaming/mte_on_pixel8/)

## Check if the current CPU supports Pointer Authentication (PAC) and Memory Tagging Extension (MTE)

```sh
$ sysctl -a | grep hw.optional.arm.FEAT
hw.optional.arm.FEAT_PAuth:  1       # Pointer Authentication (PAC) support (arm64e, since A12/M1)
hw.optional.arm.FEAT_PAuth2: 1       # Enhanced Pointer Authentication (arm64e)
hw.optional.arm.FEAT_CPA2:   0/1     # Context Pointer Authentication (CPA) support (arm64e.x1, since A19/M5)
hw.optional.arm.FEAT_MTE:    0/1     # Hardware Memory Tagging Extension (MTE) support  support (arm64e.x1)
```