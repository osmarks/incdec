const ws = require("ws")
const a = new ws("https://osmarks.net/incdec/api")
a.onopen = () => setInterval(() => a.send("i"), 1)
