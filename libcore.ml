
class String {
    fn push(self, s) {
        let selflength = strlen(self)
        let slength = strlen(s)
        let buffer: String = malloc(selflength + slength)
        sprintf(buffer, "%s%s", self, s)
        buffer
    }
}

fn ==(str1: String, str2: String) -> Bool {
    if strcmp(str1, str2) == 0 then
        true
    else
        false
}


fn str(num: Int) -> String {
    let buffer: String = malloc(22)
    sprintf(buffer, "%d", num)
    buffer
}

fn str(num: Real) -> String {
    let buffer: String = malloc(22)
    sprintf(buffer, "%f", num)
    buffer
}

fn +(s1: String, s2: String) -> String {
    s1.push(s2)
} 



class List['item] {
    let capacity = 0
    let length = 0
    let data: Buffer['item] = nil

    fn new(self) {
        self.length = 0
        self.capacity = 10
        self.data = new Buffer['item](self.capacity)
    }

    fn len(self) {
        self.length
    }

    fn resize(self, capacity) {
        self.capacity = capacity
        self.data = self.data.resize(self.capacity)
        nil
    }

    fn push(self, item: 'item) {
        if self.length + 1 >= self.capacity then
            self.resize(self.capacity + 10)
        self.data[self.length] = item
        self.length = self.length + 1
    }

    fn [](self, index: Int) {
        if index >= self.length then
            nil //raise -1
        else
            self.data[index]
    }

    fn [](self, index: Int, item: 'item) {
        if index >= self.length then
            nil //raise "IndexError: array index is out of bounds"
        else {
            self.data[index] = item
        }
    }

    fn get(self, index: Int) {
        self[index]
    }
}

