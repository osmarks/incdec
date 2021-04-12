const express = require("express")
const expressWs = require("express-ws")
const _ = require("lodash")

const db = require("level")("./db.level", {
    valueEncoding: "json"
})

const getWithDefault = async (key, def) => {
    try {
        return await db.get(key)
    } catch(e) {
        if (e.notFound) {
            return def
        } else {
            throw e
        }
    }
}

let counter = 0
setImmediate(async () => {
    counter = await getWithDefault("counter", 0)
})

setInterval(() => {
    console.log("Counter:", counter)
    db.put("counter", counter)
}, 10000)

const app = express()
const wss = expressWs(app)

app.get("/", (req, res) => {
    res.sendFile(__dirname + "/index.html")
})

const sendCounter = (ws, x) => ws.send(JSON.stringify({
    type: "value",
    value: x
}))

const transmitCounterUpdate = _.throttle(newValue => {
    wss.getWss().clients.forEach(ws => sendCounter(ws, newValue))
}, 50)

app.ws("/api", (ws, req) => {
    console.log("Connected")
    sendCounter(ws, counter)
    ws.on("message", async data => {
        const msg = JSON.parse(data)
        if (msg.type === "increment") {
            counter++
            transmitCounterUpdate(counter)
        } else if (msg.type === "decrement") {
            counter--
            transmitCounterUpdate(counter)
        }
    })
})

app.post("/inc", (req, res) => {
    counter++
    transmitCounterUpdate(counter)
    res.send(counter.toString())
})
app.post("/dec", (req, res) => {
    counter--
    transmitCounterUpdate(counter)
    res.send(counter.toString())
})

const port = parseInt(process.env.PORT) || 3709

app.listen(port, () => console.log("Listening on", port))