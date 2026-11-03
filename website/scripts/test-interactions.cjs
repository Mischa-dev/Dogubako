// Run with Playwright installed. Starts a local repository server unless BASE_URL is set.
// BASE_URL and CHROME_PATH may override the local server and browser executable.
const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const base = process.env.BASE_URL || 'http://127.0.0.1:8000';
const {spawn} = require('node:child_process');
(async () => {
  const server = process.env.BASE_URL ? null : spawn('python3',['-m','http.server','8000','--bind','127.0.0.1'],{cwd:require('node:path').resolve(__dirname,'../..'),stdio:'ignore'});
  if(server) {
    process.on('exit',()=>server.kill());
    for(let i=0;i<50;i++) {try {await fetch(base); break;} catch {await new Promise(r=>setTimeout(r,100));}}
  }
  const browser = await chromium.launch({headless:true, ...(process.env.CHROME_PATH ? {executablePath:process.env.CHROME_PATH}:{}), args:['--no-sandbox']});
  try {
    for (const [width,height] of [[1440,900],[390,844],[320,740]]) {
      for (const colorScheme of ['light','dark']) {
        const context = await browser.newContext({viewport:{width,height},colorScheme,reducedMotion:'reduce'});
        const page = await context.newPage();
        const errors=[]; page.on('pageerror',e=>errors.push(e.message));
        for (const route of ['website/','website/download/','website/tools/','website/self-hosting/','website/agents/','design/website-concepts/classic/']) {
          await page.goto(base+'/'+route); await page.waitForLoadState('networkidle');
          assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,`overflow ${route} ${width} ${colorScheme}`);
          if (route === 'website/') {
            assert.match(await page.locator('#dl-title').innerText(),/alpha is out/);
            assert.equal(await page.locator('#trailerPh').isVisible(),true);
            const step=page.locator('.step-btn').nth(3); await step.focus(); await page.keyboard.press('Enter');
            assert.equal(await step.getAttribute('aria-current'),'step');
            await page.locator('#themeBtn').click();
            assert.equal(await page.locator('html').getAttribute('data-theme'),colorScheme==='dark'?'light':'dark');
            await page.locator('#fq').fill('hash');
            assert.match(await page.locator('#fres').innerText(),/Hash/i);
            await page.locator('#fq').press('ArrowDown');
            assert.ok(await page.locator('#fq').getAttribute('aria-activedescendant'));
            await page.evaluate(async()=>{await document.fonts.load('16px "Dogubako Japanese"','道具箱');});
            assert.equal(await page.evaluate(()=>document.fonts.check('16px "Dogubako Japanese"','道具箱')),true);
          } else if(route==='website/download/') {
            assert.match(await page.locator('[data-os="mac"]').getAttribute('href'),/Dogubako_0\.1\.0-alpha\.1_aarch64\.dmg$/);
            assert.match(await page.locator('[data-os="windows"]').innerText(),/Desktop installer not published/);
            assert.match(await page.locator('body').innerText(),/CLI binaries for macOS ARM64, Windows x64 and Linux x64/);
          } else if(route==='website/tools/') {
            await page.locator('#tq').fill('sha256');
            assert.equal(await page.locator('.tl-i').filter({visible:true}).count(),await page.locator('.tl-i:not([hidden])').count());
            await page.locator('#tq').press('ArrowDown');
            assert.equal(await page.evaluate(()=>document.activeElement.matches('.tl-name a')),true);
            await page.keyboard.press('Escape');
            assert.equal(await page.locator('#tq').evaluate(e=>e===document.activeElement),true);
          } else if(route==='design/website-concepts/classic/') {
            const tabs=page.locator('.tour-tabs [role="tab"]');
            assert.equal(await tabs.evaluateAll(ts=>ts.filter(t=>t.tabIndex===0).length),1);
            await tabs.first().focus(); await page.keyboard.press('ArrowRight');
            assert.equal(await tabs.nth(1).getAttribute('aria-selected'),'true');
            const input=page.locator('#demo-file'); assert.equal(await input.getAttribute('multiple'),'');
            await input.setInputFiles([
              {name:'valid.json',mimeType:'application/json',buffer:Buffer.from('{"a":1}')},
              {name:'second.txt',mimeType:'text/plain',buffer:Buffer.from('hello')}
            ]);
            assert.equal(await page.locator('#bench-shelf [data-name="valid.json"]').count(),1);
            assert.equal(await page.locator('#bench-shelf [data-name="second.txt"]').count(),1);
            await page.locator('#bench-shelf [data-name="valid.json"]').click();
            await page.locator('#intent-list [data-tool="dev.json"]').click();
            await page.waitForFunction(()=>document.querySelector('#intent-out').textContent.includes('Valid.'));
            assert.match(await page.locator('#intent-out').innerText(),/Valid\. Indented/);
            await input.setInputFiles({name:'invalid.json',mimeType:'application/json',buffer:Buffer.from('{bad')});
            await page.locator('#intent-list [data-tool="dev.json"]').click();
            await page.waitForFunction(()=>document.querySelector('#intent-out').textContent.includes('Not valid JSON'));
            // Lazy sample is rendered to a real image, then compressed into a downloadable result.
            await page.locator('#bench-shelf [data-name="dune.jpg"]').click();
            await page.locator('#intent-list [data-tool="image.compress"]').click();
            await page.waitForFunction(()=>!!document.querySelector('#intent-out a[download]'));
            assert.match(await page.locator('#intent-out a[download]').getAttribute('download'),/dune/);
          }
        }
        assert.deepEqual(errors,[],`browser errors ${width} ${colorScheme}`);
        console.log(`PASS ${width}x${height} ${colorScheme}: five website pages and classic demo`);
        await context.close();
      }
    }
    for(const width of [1440,390]) {
      const context=await browser.newContext({viewport:{width,height:900},colorScheme:'light',reducedMotion:'no-preference'});
      const page=await context.newPage(); const errors=[];
      page.on('pageerror',e=>errors.push(e.message));
      await page.goto(base+'/website/'); await page.waitForLoadState('networkidle');
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
      await page.locator('.step-btn').nth(1).click();
      assert.equal(await page.locator('.step-btn').nth(1).getAttribute('aria-current'),'step');
      assert.deepEqual(errors,[]);
      console.log(`PASS ${width}px normal motion: homepage and chapter control`);
      await context.close();
    }
  } finally {await browser.close(); if(server) server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
