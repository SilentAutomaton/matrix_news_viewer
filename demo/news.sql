CREATE TABLE rss_item (title TEXT, url TEXT, feedurl TEXT, pubDate INTEGER, content TEXT);
INSERT INTO rss_item (title, feedurl, pubDate, content) VALUES
('AI cured cancer', 'https://world.example/rss', strftime('%s', 'now'), '<p>The model was asked nicely, thought for eleven minutes and returned a cure. Doctors say it also fixed their printer.</p>'),
('All wars are over now', 'https://world.example/rss', strftime('%s', 'now'), '<p>Every army went home for tea. Generals now argue only about whose turn it is to wash the cups.</p>'),
('Never gonna give you up', 'https://world.example/rss', strftime('%s', 'now'), '<p>Never gonna let you down, never gonna run around and desert you. The statement was confirmed by all parties.</p>'),
('Bug-free release shipped on Friday', 'https://world.example/rss', strftime('%s', 'now'), '<p>Nobody was paged over the weekend. Engineers are being treated for shock and too much free time.</p>'),
('There is no spoon shortage', 'https://world.example/rss', strftime('%s', 'now'), '<p>Officials remind citizens that there is no spoon. Therefore there can be no shortage of it.</p>'),
('Котик перешёл дорогу', 'https://city.example/rss', strftime('%s', 'now'), '<p>Рыжий котик спокойно перешёл дорогу на зелёный свет. Водители остановились и помахали ему лапками в ответ.</p>'),
('Понедельник отменён навсегда', 'https://city.example/rss', strftime('%s', 'now'), '<p>После понедельника теперь сразу идёт вторая суббота. Будильники уходят на заслуженную пенсию.</p>'),
('Wi-Fi теперь бесплатный везде', 'https://city.example/rss', strftime('%s', 'now'), '<p>Даже в лифте и в лесу. Пароль у всех один и тот же: 12345678.</p>'),
('Кофе признан овощем', 'https://city.example/rss', strftime('%s', 'now'), '<p>Учёные пересчитали зёрна и решили, что пять чашек в день — это просто здоровое питание.</p>');
