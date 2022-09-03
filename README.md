# kvass

expose local service to another machine


内网穿透工具，实现了 [frp](https://github.com/fatedier/frp) 的部分功能



## 使用

build:

```
cargo build
```

On server a.b.c.d

```
kvass server 0.0.0.0:1234
```

On client A:

```
# expose 3389
kvass main a.b.c.d:1234 127.0.0.1:3389
```

On client B:

```
kvass sub a.b.c.d:1234 127.0.0.1:4444
```


效果：把 client A 上的 3389 端口暴露到 client B 上的 4444 端口上