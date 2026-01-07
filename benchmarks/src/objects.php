<?php
$start = microtime(true);

class User {
    private $id;
    private $name;
    private $email;

    public function __construct($id, $name, $email) {
        $this->id = $id;
        $this->name = $name;
        $this->email = $email;
    }

    public function getId() { return $this->id; }
    public function getName() { return $this->name; }
    public function getEmail() { return $this->email; }
    
    public function update($name) {
        $this->name = $name;
    }
}

$users = [];
for ($i = 0; $i < 1000; $i++) {
    $u = new User($i, "User $i", "user$i@example.com");
    $u->update("Updated Name $i");
    $users[] = $u;
}

$count = 0;
foreach ($users as $u) {
    if ($u->getId() % 2 == 0) {
        $count++;
    }
}

echo "Processed " . count($users) . " objects. Even IDs: $count\n";
echo "Time: " . (microtime(true) - $start) . "s";

